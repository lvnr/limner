//! Identifiers. Unique within a document, persisted in the native format.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(
    /// Identifies a layer.
    LayerId
);
id_type!(
    /// Identifies a drawing: the strokes shown by one or more frames of paint layers.
    DrawingId
);
id_type!(
    /// Identifies an embedded picture, sound or model.
    MediaId
);
