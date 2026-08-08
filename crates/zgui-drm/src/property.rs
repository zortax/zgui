//! Object properties: what an atomic commit is built out of.
//!
//! Every modesetting object — a connector, a CRTC, a plane — carries a set of named properties.
//! An atomic commit is a list of `(object, property, value)`, so everything built on this module
//! depends on finding a property's id by its name.

use std::collections::HashMap;

use crate::device::Device;
use crate::error::{Error, Result};
use crate::ioctl;
use crate::resources::stabilise;
use crate::sys;

/// Which kind of object is being asked about.
///
/// The kernel needs to be told, because the id spaces are shared and the same number can name a
/// connector and a plane.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// A CRTC.
    Crtc,
    /// A connector.
    Connector,
    /// A plane.
    Plane,
}

impl ObjectKind {
    /// Returns the number the kernel knows this kind by.
    ///
    /// A cache of properties keys on this number as well, because the id spaces are shared.
    pub(crate) fn as_raw(self) -> u32 {
        match self {
            Self::Crtc => sys::DRM_MODE_OBJECT_CRTC,
            Self::Connector => sys::DRM_MODE_OBJECT_CONNECTOR,
            Self::Plane => sys::DRM_MODE_OBJECT_PLANE,
        }
    }
}

/// One object's properties, by name.
#[derive(Debug, Clone, Default)]
pub struct Properties {
    /// Name to property id.
    ids: HashMap<String, u32>,
    /// Name to current value.
    values: HashMap<String, u64>,
}

impl Properties {
    /// Returns the id of the property with this name.
    ///
    /// A commit names this id. A property that is not here cannot be set, and the caller decides
    /// whether that is fatal.
    pub fn id(&self, name: &str) -> Option<u32> {
        self.ids.get(name).copied()
    }

    /// Returns the current value of the property with this name.
    pub fn value(&self, name: &str) -> Option<u64> {
        self.values.get(name).copied()
    }

    /// Returns every property name this object has.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.ids.keys().map(String::as_str)
    }
}

impl Device {
    /// Reads the properties of one object.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses, and [`Error::Unusable`] when the count
    /// kept moving.
    pub fn properties(&self, object: u32, kind: ObjectKind) -> Result<Properties> {
        stabilise(
            || format!("object {object} changed under every attempt to read its properties"),
            || {
                let mut counts = sys::drm_mode_obj_get_properties {
                    obj_id: object,
                    obj_type: kind.as_raw(),
                    ..Default::default()
                };
                ioctl::issue(self.fd(), ioctl::MODE_OBJ_GETPROPERTIES, &mut counts)?;

                let mut ids = vec![0_u32; counts.count_props as usize];
                let mut values = vec![0_u64; counts.count_props as usize];
                let mut filled = sys::drm_mode_obj_get_properties {
                    obj_id: object,
                    obj_type: kind.as_raw(),
                    props_ptr: ids.as_mut_ptr() as u64,
                    prop_values_ptr: values.as_mut_ptr() as u64,
                    count_props: counts.count_props,
                };
                ioctl::issue(self.fd(), ioctl::MODE_OBJ_GETPROPERTIES, &mut filled)?;

                if filled.count_props != counts.count_props {
                    return Ok(None);
                }

                let mut properties = Properties::default();
                for (id, value) in ids.into_iter().zip(values) {
                    let name = self.property_name(id)?;
                    properties.ids.insert(name.clone(), id);
                    properties.values.insert(name, value);
                }
                Ok(Some(properties))
            },
        )
    }

    /// Returns the name of one property.
    fn property_name(&self, id: u32) -> Result<String> {
        let mut request = sys::drm_mode_get_property {
            prop_id: id,
            ..Default::default()
        };
        ioctl::issue(self.fd(), ioctl::MODE_GETPROPERTY, &mut request)?;

        // The name is a fixed array padded with zeros. Anything after the first zero is not part
        // of it, and a name that fills the array has no terminator at all.
        let bytes: Vec<u8> = request
            .name
            .iter()
            .take_while(|byte| **byte != 0)
            // `c_char` is signed on x86_64 and unsigned on aarch64, so this cast converts on the
            // first and is the no-op `unnecessary_cast` rejects on the second.
            .map(|byte| *byte as u8)
            .collect();
        String::from_utf8(bytes)
            .map_err(|_| Error::Unusable(format!("property {id} has a name that is not text")))
    }

    /// Creates a blob property holding `bytes`, returning its id.
    ///
    /// A mode is handed to an atomic commit this way: the timings go into a blob, and the blob's
    /// id is what `MODE_ID` is set to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses, and [`Error::Unusable`] when `bytes` is
    /// longer than the interface can describe.
    pub fn create_blob(&self, bytes: &[u8]) -> Result<u32> {
        let mut request = sys::drm_mode_create_blob {
            data: bytes.as_ptr() as u64,
            length: u32::try_from(bytes.len()).map_err(|_| {
                Error::Unusable("a property blob longer than the interface allows".to_owned())
            })?,
            blob_id: 0,
        };
        ioctl::issue(self.fd(), ioctl::MODE_CREATEPROPBLOB, &mut request)?;
        Ok(request.blob_id)
    }

    /// Destroys the blob property `id` names.
    ///
    /// The header states that a blob may be released "as soon as the commit has been issued,
    /// without waiting for it to complete", so a mode blob is safe to destroy while the mode it
    /// describes is on screen. The kernel keeps its own reference for as long as it needs one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses, which is how an id it does not know is
    /// answered.
    pub fn destroy_blob(&self, id: u32) -> Result<()> {
        let mut request = sys::drm_mode_destroy_blob { blob_id: id };
        ioctl::issue(self.fd(), ioctl::MODE_DESTROYPROPBLOB, &mut request)
    }
}
