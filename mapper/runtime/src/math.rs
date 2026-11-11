//! Pure source-coordinate decoding. Python owns only discovery/metadata; native
//! transforms, camera validation and screen projection do not hold its GIL.

pub fn matvec(m: &[f64; 16], v: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|r| (0..4).map(|c| m[c * 4 + r] * v[c]).sum())
}
pub fn trs_chain(raw: &[u8], local: Option<[f64; 3]>) -> Option<[f64; 3]> {
    if raw.is_empty() || raw.len() % 48 != 0 || raw.len() > 128 * 48 {
        return None;
    }
    let mut point = local;
    for item in raw.chunks_exact(48) {
        let v: [f64; 12] = std::array::from_fn(|i| {
            f32::from_le_bytes(item[i * 4..i * 4 + 4].try_into().unwrap()) as f64
        });
        if [0, 1, 2, 4, 5, 6, 7, 8, 9, 10]
            .iter()
            .any(|&i| !v[i].is_finite() || v[i].abs() >= 1e6)
            || ((4..8).map(|i| v[i] * v[i]).sum::<f64>() - 1.).abs() > 0.01
        {
            return None;
        }
        point = Some(if let Some(p) = point {
            let (x, y, z) = (p[0] * v[8], p[1] * v[9], p[2] * v[10]);
            let (qx, qy, qz, qw) = (v[4], v[5], v[6], v[7]);
            let (tx, ty, tz) = (
                2. * (qy * z - qz * y),
                2. * (qz * x - qx * z),
                2. * (qx * y - qy * x),
            );
            [
                x + qw * tx + qy * tz - qz * ty + v[0],
                y + qw * ty + qz * tx - qx * tz + v[1],
                z + qw * tz + qx * ty - qy * tx + v[2],
            ]
        } else {
            [v[0], v[1], v[2]]
        });
    }
    point
}
pub fn matrix_offset(raw: &[u8], orthographic: bool) -> Option<usize> {
    if raw.len() < 128 || raw.len() > 1048576 {
        return None;
    }
    let mut found = None;
    for off in (0..=raw.len() - 128).step_by(4) {
        let v: [f64; 32] = std::array::from_fn(|i| {
            f32::from_le_bytes(raw[off + i * 4..off + i * 4 + 4].try_into().unwrap()) as f64
        });
        if v.iter().any(|x| !x.is_finite() || x.abs() >= 1e6) {
            continue;
        }
        let p = &v[16..];
        let invalid_shape = if orthographic {
            !(-100. < p[10] && p[10] < 0.) || p[11].abs() > 1e-5 || (p[15] - 1.).abs() > 1e-5
        } else {
            !(-1.1 < p[10] && p[10] < -0.99)
                || (p[11] + 1.).abs() > 1e-5
                || !(-10. < p[14] && p[14] < 0.)
                || p[15].abs() > 1e-5
        };
        if [3, 7, 11].iter().any(|&i| v[i].abs() > 1e-5)
            || (v[15] - 1.).abs() > 1e-5
            || !(0.05 < p[0] && p[0] < 20. && 0.05 < p[5] && p[5] < 20.)
            || invalid_shape
            || [1, 2, 3, 4, 6, 7, 12, 13]
                .iter()
                .any(|&i| p[i].abs() > 1e-5)
        {
            continue;
        }
        if (0..3).any(|a| {
            (0..3).any(|b| {
                ((0..3).map(|r| v[a * 4 + r] * v[b * 4 + r]).sum::<f64>()
                    - if a == b { 1. } else { 0. })
                .abs()
                    > 1e-3
            })
        }) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(off);
    }
    found
}
pub fn project(
    view: &[f64; 16],
    projection: &[f64; 16],
    point: [f64; 3],
    allow_behind: bool,
) -> Option<[f64; 2]> {
    let clip = matvec(projection, matvec(view, [point[0], point[1], point[2], 1.]));
    if clip.iter().any(|v| !v.is_finite()) {
        return None;
    }
    if clip[3] <= 1e-6 {
        return if allow_behind { Some([-1., -1.]) } else { None };
    }
    Some([(clip[0] / clip[3] + 1.) / 2., (1. - clip[1] / clip[3]) / 2.])
}
pub fn camera_pose(view: &[f64; 16], trs: &[u8]) -> bool {
    let Some(eye) = trs_chain(trs, None) else {
        return false;
    };
    let mapped = matvec(view, [eye[0], eye[1], eye[2], 1.]);
    let tolerance = 0.001 + eye.iter().map(|x| x.abs()).fold(0., f64::max) * 2e-6;
    if mapped[..3]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > tolerance)
    {
        return false;
    }
    for axis in 0..3 {
        let mut local = [0.; 3];
        local[axis] = 1.;
        let Some(p) = trs_chain(trs, Some(local)) else {
            return false;
        };
        let d: [f64; 3] = std::array::from_fn(|i| p[i] - eye[i]);
        let len = d.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !len.is_finite() || len <= 1e-8 {
            return false;
        }
        let projected = matvec(view, [d[0] / len, d[1] / len, d[2] / len, 0.]);
        if (0..3).any(|i| {
            (projected[i]
                - if i == axis {
                    if i == 2 { -1. } else { 1. }
                } else {
                    0.
                })
            .abs()
                > 3e-4
        }) {
            return false;
        }
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> Vec<u8> {
        [
            t[0],
            t[1],
            t[2],
            f32::NAN,
            q[0],
            q[1],
            q[2],
            q[3],
            s[0],
            s[1],
            s[2],
            f32::NAN,
        ]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect()
    }
    #[test]
    fn transform_hierarchy_and_ignored_simd_padding() {
        let mut b = trs([1., 2., 3.], [0., 0., 0., 1.], [1.; 3]);
        b.extend(trs([5., 0., 0.], [0., 0., 0., 1.], [2., 3., 4.]));
        assert_eq!(trs_chain(&b, None), Some([7., 6., 12.]));
        assert_eq!(trs_chain(&b, Some([1.; 3])), Some([9., 9., 16.]));
    }
    #[test]
    fn invalid_rotation_or_scale_rejected() {
        let b = trs([0.; 3], [0.; 4], [1.; 3]);
        assert!(trs_chain(&b, None).is_none());
        assert!(trs_chain(&[], None).is_none());
    }
    #[test]
    fn camera_requires_all_axes_and_position() {
        let b = trs([5., 6., 7.], [0., 0., 0., 1.], [1.; 3]);
        let mut v = [0.; 16];
        v[0] = 1.;
        v[5] = 1.;
        v[10] = -1.;
        v[15] = 1.;
        v[12] = -5.;
        v[13] = -6.;
        v[14] = 7.;
        assert!(camera_pose(&v, &b));
        v[14] = 0.;
        assert!(!camera_pose(&v, &b));
        v[14] = 7.;
        v[0] = -1.;
        assert!(!camera_pose(&v, &b));
    }
}
