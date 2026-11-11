#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Clone, Debug)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub readable: bool,
    pub writable: bool,
    pub name: String,
}

/// Limits keep one sampling transaction bounded across all platform adapters.
pub const MAX_READ_SPANS: usize = 64;
pub const MAX_READ_BYTES: usize = 1024 * 1024;

pub trait Memory {
    fn regions(&self) -> Result<Vec<Region>, String>;
    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>, String>;
    fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<(), String>;
    /// Read every span, in order, into one flat destination. This is not an
    /// atomic game snapshot: callers retain their existing before/after fences.
    /// An error invalidates the whole destination, including any copied prefix.
    fn read_vectored(&self, spans: &[(u64, usize)], bytes: &mut [u8]) -> Result<(), String> {
        validate_read_layout(spans, bytes.len())?;
        let mut offset = 0;
        for &(address, length) in spans {
            self.read_into(address, &mut bytes[offset..offset + length])?;
            offset += length;
        }
        Ok(())
    }
}

/// Validate before any adapter starts copying, so malformed layouts cannot
/// expose an accepted partial result or cause arithmetic/slice overflow.
pub(crate) fn validate_read_layout(
    spans: &[(u64, usize)],
    destination_len: usize,
) -> Result<(), String> {
    if spans.len() > MAX_READ_SPANS || destination_len > MAX_READ_BYTES {
        return Err("Read vector exceeds bounded transaction".into());
    }
    let total = spans
        .iter()
        .try_fold(0usize, |total, &(address, length)| {
            address.checked_add(length as u64)?;
            total.checked_add(length)
        })
        .ok_or("Invalid memory vector span")?;
    if total != destination_len {
        return Err("Read vector destination size mismatch".into());
    }
    Ok(())
}

/// Recheck retained sample bytes without discarding duplicate observations or
/// moving any sample-phase boundary. One buffer is reused across bounded batches.
pub fn matches_reads<M: Memory + ?Sized>(
    memory: &M,
    expected: &[(u64, Vec<u8>)],
) -> Result<bool, String> {
    let mut scratch = Vec::new();
    let mut spans = [(0u64, 0usize); MAX_READ_SPANS];
    let mut cursor = 0;
    while cursor < expected.len() {
        let mut count = 0;
        let mut total = 0;
        while count < MAX_READ_SPANS && cursor + count < expected.len() {
            let (address, bytes) = &expected[cursor + count];
            if bytes.len() > MAX_READ_BYTES {
                return Err("Retained memory span exceeds bounded read".into());
            }
            if total + bytes.len() > MAX_READ_BYTES {
                break;
            }
            spans[count] = (*address, bytes.len());
            total += bytes.len();
            count += 1;
        }
        scratch.resize(total, 0);
        memory.read_vectored(&spans[..count], &mut scratch)?;
        let mut offset = 0;
        for (_, bytes) in &expected[cursor..cursor + count] {
            if scratch[offset..offset + bytes.len()] != bytes[..] {
                return Ok(false);
            }
            offset += bytes.len();
        }
        cursor += count;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct TracedMemory {
        reads: RefCell<Vec<u64>>,
        fail_at: Option<u64>,
    }
    impl Memory for TracedMemory {
        fn regions(&self) -> Result<Vec<Region>, String> {
            Ok(Vec::new())
        }
        fn read(&self, address: u64, length: usize) -> Result<Vec<u8>, String> {
            let mut bytes = vec![0; length];
            self.read_into(address, &mut bytes)?;
            Ok(bytes)
        }
        fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<(), String> {
            self.reads.borrow_mut().push(address);
            if self.fail_at == Some(address) {
                return Err("owned fixture fault".into());
            }
            bytes.fill(address as u8);
            Ok(())
        }
    }
    #[test]
    fn vector_fences_preserve_order_duplicates_and_separate_phases() {
        let memory = TracedMemory {
            reads: RefCell::new(Vec::new()),
            fail_at: None,
        };
        let phase: Vec<_> = (0..130)
            .map(|i| ((i % 7) as u64, vec![(i % 7) as u8; 8]))
            .collect();
        assert!(matches_reads(&memory, &phase).unwrap());
        assert_eq!(
            *memory.reads.borrow(),
            phase.iter().map(|r| r.0).collect::<Vec<_>>()
        );
        assert!(matches_reads(&memory, &phase).unwrap());
        assert_eq!(memory.reads.borrow().len(), 260);
        let wrong = [(3, vec![4; 8])];
        assert!(!matches_reads(&memory, &wrong).unwrap());
    }
    #[test]
    fn partial_vector_failure_cannot_accept_copied_prefix() {
        let memory = TracedMemory {
            reads: RefCell::new(Vec::new()),
            fail_at: Some(2),
        };
        let expected = [(1, vec![1; 8]), (2, vec![2; 8]), (3, vec![3; 8])];
        assert!(matches_reads(&memory, &expected).is_err());
        assert_eq!(*memory.reads.borrow(), vec![1, 2]);
    }
    #[test]
    fn malformed_vector_rejected_before_reading_and_large_fences_chunked() {
        let memory = TracedMemory {
            reads: RefCell::new(Vec::new()),
            fail_at: None,
        };
        assert!(memory.read_vectored(&[(1, 4)], &mut [0; 3]).is_err());
        assert!(memory.read_vectored(&[(u64::MAX, 2)], &mut [0; 2]).is_err());
        assert!(
            memory
                .read_vectored(&[(1, 0); MAX_READ_SPANS + 1], &mut [])
                .is_err()
        );
        assert!(memory.reads.borrow().is_empty());
        let expected = [(1, vec![1; MAX_READ_BYTES]), (2, vec![2; 8])];
        assert!(matches_reads(&memory, &expected).unwrap());
        assert!(matches_reads(&memory, &[(1, vec![1; MAX_READ_BYTES + 1])]).is_err());
    }
}
