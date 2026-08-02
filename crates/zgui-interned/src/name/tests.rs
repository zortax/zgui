//! Tests shared by every interned name type.

use proptest::prelude::*;

use crate::name::{AttrName, ClassName, CustomPropertyName, ElementName, Ident};
use crate::{Atom, CheapCloneStr};

/// Asserts of one name type everything that holds of all of them.
macro_rules! shared_behaviour {
    ($name:ident, $module:ident) => {
        mod $module {
            use super::*;

            proptest! {
                #[test]
                fn equal_text_is_one_identity(text in ".{0,24}") {
                    let first = $name::new(&text);
                    let second = $name::new(&text);
                    prop_assert!(first.is(second));
                    prop_assert_eq!(first, second);
                    prop_assert_eq!(first.as_str(), text.as_str());
                    prop_assert_eq!(first.len(), text.len());
                    prop_assert_eq!(first.is_empty(), text.is_empty());
                }

                #[test]
                fn every_conversion_reaches_the_same_name(text in ".{0,24}") {
                    let direct = $name::new(&text);
                    let borrowed: $name = text.as_str().into();
                    let owned: $name = text.clone().into();
                    let from_atom: $name = Atom::new(&text).into();
                    prop_assert!(direct.is(borrowed));
                    prop_assert!(direct.is(owned));
                    prop_assert!(direct.is(from_atom));
                    prop_assert_eq!(Atom::from(direct), Atom::new(&text));
                    prop_assert_eq!($name::from_atom(direct.atom()), direct);
                }

                #[test]
                fn comparison_against_a_plain_string_reads_the_text(text in ".{0,24}") {
                    prop_assert!($name::new(&text) == text.as_str());
                }
            }

            #[test]
            fn a_name_is_one_pointer_wide_and_leaves_room_for_a_niche() {
                assert_eq!(size_of::<$name>(), size_of::<usize>());
                assert_eq!(size_of::<Option<$name>>(), size_of::<usize>());
            }

            #[test]
            fn the_default_name_is_empty() {
                assert!($name::default().is_empty());
                assert_eq!($name::default().as_str(), "");
            }

            #[test]
            fn debug_names_the_type_and_shows_the_text() {
                let name = $name::new("value");
                assert_eq!(
                    format!("{name:?}"),
                    concat!(stringify!($name), "(\"value\")")
                );
                assert_eq!(name.to_string(), "value");
            }

            /// The bound set a layout engine's custom-identifier parameter requires. Losing any
            /// one of these traits is a break that only shows up in a downstream crate.
            #[test]
            fn the_type_is_a_cheaply_cloned_string() {
                fn require<T: CheapCloneStr>() {}
                require::<$name>();
            }
        }
    };
}

shared_behaviour!(ElementName, element_name);
shared_behaviour!(AttrName, attr_name);
shared_behaviour!(ClassName, class_name);
shared_behaviour!(Ident, ident);
shared_behaviour!(CustomPropertyName, custom_property_name);

#[test]
fn names_of_different_kinds_do_not_mix_even_over_the_same_text() {
    // They compare equal only through the atom they share, which is the one place the distinction
    // is deliberately dropped.
    let element = ElementName::new("colour");
    let attribute = AttrName::new("colour");
    assert_eq!(element.atom(), attribute.atom());
    assert_eq!(element.as_str(), attribute.as_str());
}

proptest! {
    /// A custom property name round-trips through its declaration form.
    #[test]
    fn a_custom_property_round_trips_through_its_declaration(name in "[a-zA-Z0-9-]{1,16}") {
        let declaration = format!("--{name}");
        let parsed = CustomPropertyName::parse(&declaration).expect("a custom property name");
        prop_assert_eq!(parsed.as_str(), name.as_str());
        prop_assert_eq!(parsed.to_declaration(), declaration);
    }
}

#[test]
fn a_declaration_that_is_not_a_custom_property_is_rejected() {
    assert!(CustomPropertyName::parse("--").is_none());
    assert!(CustomPropertyName::parse("-x").is_none());
    assert!(CustomPropertyName::parse("colour").is_none());
    assert!(CustomPropertyName::parse("").is_none());
}
