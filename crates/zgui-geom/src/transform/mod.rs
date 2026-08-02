//! Transforms, from the two-dimensional case to the full CSS 3D matrix.
//!
//! [`Affine2`] covers everything that keeps the drawing in its plane: translation, scale,
//! rotation, skew and any composition of them. It is six floats and it has a cheap inverse, so
//! it is what scrolling, clipping and hit testing use.
//!
//! [`Matrix4`] covers the rest of CSS `transform`: perspective, `rotate3d`, `translateZ`. It is a
//! column-major 4x4 matrix, laid out exactly as a shader expects to receive one.
//!
//! [`Decomposed`] is a [`Matrix4`] taken apart into translation, scale, skew, perspective and a
//! rotation quaternion. Interpolating two transforms — which is what an animation between two
//! `transform` values does — has to happen on the parts, because interpolating the matrices
//! themselves collapses rotations and produces visible nonsense.

pub mod affine2;
pub mod bounds;
pub mod decompose;
pub mod matrix4;

pub use crate::transform::affine2::Affine2;
pub use crate::transform::bounds::transformed_bounds;
pub use crate::transform::decompose::Decomposed;
pub use crate::transform::matrix4::Matrix4;
