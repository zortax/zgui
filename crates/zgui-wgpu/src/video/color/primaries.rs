//! Colour primaries, and the linear map from one set to the compositor's.

/// The red, green and blue a frame's colour is mixed from. Every set uses the D65 white point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorPrimaries {
    /// ITU-R BT.709 and sRGB.
    #[default]
    Bt709,
    /// SMPTE 170M: 525-line standard-definition video.
    Bt601,
    /// ITU-R BT.470 System B/G: 625-line standard-definition video.
    Bt470Bg,
    /// ITU-R BT.2020 and BT.2100.
    Bt2020,
    /// SMPTE EG 432-1: Display P3.
    DisplayP3,
}

/// The CIE 1931 xy chromaticity of the D65 white point.
const D65: [f64; 2] = [0.3127, 0.3290];

impl ColorPrimaries {
    /// The xy chromaticities of red, green and blue.
    fn chromaticities(self) -> [[f64; 2]; 3] {
        match self {
            Self::Bt709 => [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060]],
            Self::Bt601 => [[0.630, 0.340], [0.310, 0.595], [0.155, 0.070]],
            Self::Bt470Bg => [[0.640, 0.330], [0.290, 0.600], [0.150, 0.060]],
            Self::Bt2020 => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]],
            Self::DisplayP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]],
        }
    }

    /// The matrix from linear RGB on these primaries to CIE XYZ.
    fn to_xyz(self) -> Matrix {
        let xyz = |[x, y]: [f64; 2]| [x / y, 1.0, (1.0 - x - y) / y];
        let [r, g, b] = self.chromaticities().map(xyz);
        let columns = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
        // Scale each primary so that equal amounts of all three make the white point.
        let white = xyz(D65);
        let scale = mul_vec(&inverse(&columns), white);
        columns.map(|row| [row[0] * scale[0], row[1] * scale[1], row[2] * scale[2]])
    }

    /// The luminance weight of each primary: the Y row of [`to_xyz`](Self::to_xyz).
    pub(crate) fn luminance(self) -> [f32; 3] {
        self.to_xyz()[1].map(|v| v as f32)
    }
}

/// The linear map from one frame's primaries to BT.709.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Gamut(pub [[f32; 3]; 3]);

impl Gamut {
    /// The map from linear RGB on `source` to linear RGB on BT.709.
    pub(crate) fn to_bt709(source: ColorPrimaries) -> Self {
        let target = inverse(&ColorPrimaries::Bt709.to_xyz());
        let map = mul(&target, &source.to_xyz());
        Self(map.map(|row| row.map(|v| v as f32)))
    }
}

/// A 3×3 matrix, row-major.
type Matrix = [[f64; 3]; 3];

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn mul_vec(a: &Matrix, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| (0..3).map(|k| a[i][k] * v[k]).sum())
}

fn inverse(m: &Matrix) -> Matrix {
    let cofactor = |r: usize, c: usize| {
        let (r0, r1) = ((r + 1) % 3, (r + 2) % 3);
        let (c0, c1) = ((c + 1) % 3, (c + 2) % 3);
        m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
    };
    let determinant: f64 = (0..3).map(|c| m[0][c] * cofactor(0, c)).sum();
    std::array::from_fn(|i| std::array::from_fn(|j| cofactor(j, i) / determinant))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [[f32; 3]; 3], b: [[f32; 3]; 3], tolerance: f32) -> bool {
        a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .all(|(x, y)| (x - y).abs() <= tolerance)
    }

    #[test]
    fn bt709_maps_onto_itself() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(close(
            Gamut::to_bt709(ColorPrimaries::Bt709).0,
            identity,
            1e-6
        ));
    }

    #[test]
    fn bt2020_matches_the_bt2087_matrix() {
        // ITU-R BT.2087, the linear BT.2020 to BT.709 conversion.
        let published = [
            [1.6605, -0.5876, -0.0728],
            [-0.1246, 1.1329, -0.0083],
            [-0.0182, -0.1006, 1.1187],
        ];
        assert!(close(
            Gamut::to_bt709(ColorPrimaries::Bt2020).0,
            published,
            5e-4
        ));
    }

    #[test]
    fn white_stays_white_on_every_set() {
        for source in [
            ColorPrimaries::Bt601,
            ColorPrimaries::Bt470Bg,
            ColorPrimaries::Bt2020,
            ColorPrimaries::DisplayP3,
        ] {
            let map = Gamut::to_bt709(source).0;
            for row in map {
                assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{source:?}");
            }
        }
    }

    #[test]
    fn luminance_weights_are_the_matrix_constants() {
        let [r, _, b] = ColorPrimaries::Bt709.luminance();
        assert!((r - 0.2126).abs() < 1e-4 && (b - 0.0722).abs() < 1e-4);
        let [r, _, b] = ColorPrimaries::Bt2020.luminance();
        assert!((r - 0.2627).abs() < 1e-4 && (b - 0.0593).abs() < 1e-4);
    }
}
