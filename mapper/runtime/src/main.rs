use clbb_mapper::{
    platform::linux::{ProcessMemory, now_ns},
    source::Source,
};
fn main() -> Result<(), String> {
    let pid: u32 = std::env::args()
        .nth(1)
        .ok_or("Expected owned ATL pid")?
        .parse()
        .map_err(|_| "Invalid pid")?;
    let mut source = Source::new(ProcessMemory::diagnostic(pid)?, format!("clbb:{pid}"))?;
    let size = [
        std::env::args()
            .nth(2)
            .and_then(|v| v.parse().ok())
            .unwrap_or(1308.),
        std::env::args()
            .nth(3)
            .and_then(|v| v.parse().ok())
            .unwrap_or(713.),
    ];
    let started = std::time::Instant::now();
    let sample = source.sample(now_ns(), size)?;
    println!(
        "CLBB_SOURCE_OK hero={} prepare_ms={:.1}",
        sample.frame.hero_id.int(),
        started.elapsed().as_secs_f64() * 1000.
    );
    println!(
        "AIM skill={} visual={} input={} processed={}",
        sample.frame.skill_id.int(),
        sample.aim.is_some(),
        sample.frame.logical.input_ready.get(),
        sample.frame.logical.ready.get()
    );
    for b in &sample.frame.ui.buttons[..sample.frame.ui.button_count as usize] {
        println!(
            "BUTTON {} skill={} xy={:.4},{:.4} radius={:.4},{:.4}",
            b.slot.as_str(),
            b.skill_id.int(),
            b.press_uv.values[0].value,
            b.press_uv.values[1].value,
            b.radius_uv.values[0].value,
            b.radius_uv.values[1].value
        );
    }
    let started = std::time::Instant::now();
    let mut valid = 0;
    for _ in 0..100 {
        if source.sample(now_ns(), size).is_ok() {
            valid += 1;
        }
    }
    println!(
        "CLBB_SOURCE_TIMING samples=100 valid={valid} mean_ms={:.3}",
        started.elapsed().as_secs_f64() * 10.
    );
    Ok(())
}
