//! What a line tells an assistive technology.

use accesskit::{Node, Role, TextDirection};
use zgui_geom::CssPx;
use zgui_text::{ClusterGeometry, TextRunAttributes};

#[test]
fn a_text_run_describes_itself_consistently() {
    let text = "héllo";
    let clusters: Vec<ClusterGeometry> = [(0, 1), (1, 2), (3, 1), (4, 1), (5, 1)]
        .iter()
        .scan(0.0f32, |offset, (start, length)| {
            let cluster = ClusterGeometry {
                text: *start..*start + *length,
                offset: CssPx(*offset),
                advance: CssPx(8.0),
            };
            *offset += 8.0;
            Some(cluster)
        })
        .collect();

    let attributes =
        TextRunAttributes::from_clusters(&clusters, vec![0], TextDirection::LeftToRight);

    assert!(
        attributes.is_consistent(text),
        "the arrays agree with the text"
    );
    assert_eq!(attributes.character_lengths, [1, 2, 1, 1, 1]);
    assert_eq!(attributes.character_positions, [0.0, 8.0, 16.0, 24.0, 32.0]);
    assert_eq!(attributes.character_widths, [8.0; 5]);

    let mut node = Node::new(Role::TextRun);
    attributes.apply(&mut node);
    assert_eq!(node.character_lengths(), [1, 2, 1, 1, 1]);
    assert_eq!(node.text_direction(), Some(TextDirection::LeftToRight));
}

/// The consistency check is not vacuous: a run whose lengths do not sum to its text fails it.
#[test]
fn a_text_run_whose_lengths_do_not_sum_is_rejected() {
    let clusters = [ClusterGeometry {
        text: 0..1,
        offset: CssPx::ZERO,
        advance: CssPx(8.0),
    }];
    let attributes =
        TextRunAttributes::from_clusters(&clusters, Vec::new(), TextDirection::LeftToRight);

    assert!(attributes.is_consistent("a"));
    assert!(
        !attributes.is_consistent("ab"),
        "one cluster is not two bytes"
    );
}
