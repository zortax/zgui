//! Which device pixel a translated box is drawn at.

use zgui_geom::Matrix4;

use crate::probe::{box_named, own_fragment};
use crate::support::{Element, Fixture, lay_out, measurer};

/// The matrix a lone `card` under `declarations` is drawn through.
fn card_matrix(declarations: &str) -> Matrix4 {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("card")]),
        &format!(
            "root {{ display: block; width: 300px }}
             card {{ display: block; {declarations} }}"
        ),
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 300.0, 300.0);
    let card = box_named(&store, &fixture, "card");
    let space = own_fragment(&store, card)
        .transform
        .expect("every fragment names a coordinate system");
    frame
        .spatial
        .resolve(space)
        .expect("a live coordinate system")
}

/// The matrix's horizontal and vertical translation.
fn translation(matrix: &Matrix4) -> (f32, f32) {
    (matrix.get(0, 3), matrix.get(1, 3))
}

#[test]
fn a_box_pulled_back_by_half_its_odd_size_lands_on_whole_pixels() {
    // Half of 101 by 41 is a half pixel off the grid in both directions.
    let matrix = card_matrix("width: 101px; height: 41px; transform: translate(-50%, -50%)");
    let (x, y) = translation(&matrix);
    assert_eq!(x, x.round(), "the horizontal pull-back is {x}");
    assert_eq!(y, y.round(), "the vertical pull-back is {y}");
}

#[test]
fn a_fractional_translation_rounds_to_the_nearest_pixel() {
    let matrix = card_matrix("height: 10px; transform: translate(0.3px, 0.7px)");
    assert_eq!(translation(&matrix), (0.0, 1.0));
}

#[test]
fn a_scaled_box_keeps_its_exact_matrix() {
    // Scaled about its centre at (50.5, 20.5), the box's corner moves to a fraction of a pixel.
    let matrix = card_matrix("width: 101px; height: 41px; transform: scale(0.5)");
    assert_eq!(matrix.get(0, 0), 0.5);
    assert_eq!(translation(&matrix), (25.25, 10.25));
}
