//! Bringing a picture, a sound or a model into the document as a new layer.
//!
//! Reading and decoding the file is the caller's business (the core does no I/O).
//! What arrives here is the content, ready to embed.

use super::{CommandError, Session};
use crate::model::{
    Attenuation, LayerId, LayerKind, MediaData, MediaKind, Model, Picture, Projection, Shading,
    Sound, SoundKind, SoundModifier,
};
use crate::ops::layers;

/// What kind of layer an imported file becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaLayerKind {
    Picture,
    Sound {
        /// How the sound starts out: a point in space or the same everywhere.
        kind: SoundKind,
    },
    Model,
}

impl MediaLayerKind {
    fn media_kind(self) -> MediaKind {
        match self {
            MediaLayerKind::Picture => MediaKind::Image,
            MediaLayerKind::Sound { .. } => MediaKind::Audio,
            MediaLayerKind::Model => MediaKind::Mesh,
        }
    }

    /// The name of the edit in the undo menu.
    pub fn label(self) -> &'static str {
        match self {
            MediaLayerKind::Picture => "Import picture",
            MediaLayerKind::Sound { .. } => "Import sound",
            MediaLayerKind::Model => "Import model",
        }
    }
}

/// An imported file, decoded as far as the document stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct NewMedia {
    pub kind: MediaLayerKind,
    /// The file's name, kept with the media for display.
    pub file_name: String,
    /// Where the file was read from, kept on the layer.
    pub source_path: String,
    pub data: MediaData,
}

/// True when `data` is something a layer of `kind` can show or play.
fn fits(kind: MediaLayerKind, data: &MediaData) -> bool {
    match (kind, data) {
        (MediaLayerKind::Picture, MediaData::RawImage { .. }) => true,
        (MediaLayerKind::Picture, MediaData::Encoded { format, .. }) => {
            matches!(format.as_str(), "png" | "jpg")
        }
        (MediaLayerKind::Sound { .. }, MediaData::RawAudio { .. }) => true,
        (MediaLayerKind::Sound { .. }, MediaData::Encoded { format, .. }) => {
            matches!(format.as_str(), "wav" | "ogg" | "mp3")
        }
        (MediaLayerKind::Model, MediaData::Mesh(_)) => true,
        _ => false,
    }
}

/// The layer name for a file: its name without the folder and the extension.
fn layer_name(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = match base.rfind('.') {
        Some(dot) if dot > 0 => &base[..dot],
        _ => base,
    };
    layers::clean_name(stem)
}

impl Session {
    /// Adds a picture, sound or model layer holding `media`, placed where a new layer
    /// goes, named after the file and selected. One undo step.
    pub fn add_media_layer(&mut self, media: NewMedia) -> Result<LayerId, CommandError> {
        if !fits(media.kind, &media.data) {
            return Err(CommandError::invalid(
                "the file's content does not fit the kind of layer it was imported as",
            ));
        }
        self.edit(media.kind.label(), |session| {
            let (parent, index) = session.default_insertion_point();
            let clip_start = if session.options.create_layers_at_playhead {
                session
                    .doc
                    .settings
                    .rate
                    .snap_floor(session.evaluate().child_time(&session.doc, parent))
                    .max(0)
            } else {
                0
            };
            let name = layers::unique_name(&session.doc, parent, &layer_name(&media.file_name));
            let doc = session.doc_mut();
            let id = doc.add_media(media.file_name, media.kind.media_kind(), media.data);
            let kind = match media.kind {
                MediaLayerKind::Picture => LayerKind::Picture(Picture {
                    media: Some(id),
                    projection: Projection::Flat,
                    viewer_locked: false,
                    source_path: media.source_path,
                }),
                MediaLayerKind::Sound { kind } => LayerKind::Sound(Sound {
                    media: Some(id),
                    kind,
                    gain: 1.0,
                    looped: false,
                    attenuation: Attenuation::default(),
                    modifier: SoundModifier::None,
                    source_path: media.source_path,
                }),
                MediaLayerKind::Model => LayerKind::Model(Model {
                    media: Some(id),
                    shading: Shading::Unlit,
                    wireframe: false,
                    source_path: media.source_path,
                }),
            };
            let layer = doc.new_layer(name, kind, clip_start);
            let layer = doc.insert_layer(layer, parent, index)?;
            session.set_current_layer(layer);
            Ok(layer)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, MeshData};
    use std::sync::Arc;

    fn png() -> NewMedia {
        NewMedia {
            kind: MediaLayerKind::Picture,
            file_name: "Sky at dusk.png".into(),
            source_path: "/pictures/Sky at dusk.png".into(),
            data: MediaData::Encoded {
                format: "png".into(),
                bytes: Arc::from(&[1u8, 2, 3][..]),
            },
        }
    }

    fn names(session: &Session) -> Vec<String> {
        let doc = session.doc();
        doc.children(doc.root())
            .iter()
            .map(|c| doc.layer(*c).unwrap().name.clone())
            .collect()
    }

    #[test]
    fn an_imported_picture_is_a_selected_layer_after_the_current_one_and_one_undo_step() {
        let mut session = Session::new(Document::new());
        let id = session.add_media_layer(png()).unwrap();
        assert_eq!(names(&session), ["Spawn Area", "Paint", "Sky at dusk"]);
        assert_eq!(session.current_layer(), Some(id));
        assert_eq!(session.history().undo_label(), Some("Import picture"));
        let layer = session.doc().layer(id).unwrap();
        let LayerKind::Picture(picture) = &layer.kind else {
            panic!("not a picture: {:?}", layer.kind)
        };
        assert_eq!(picture.projection, Projection::Flat);
        assert_eq!(picture.source_path, "/pictures/Sky at dusk.png");
        let media = session.doc().media(picture.media.unwrap()).unwrap();
        assert_eq!(media.name, "Sky at dusk.png");
        assert_eq!(media.kind, MediaKind::Image);
        assert!(session.is_dirty());
        session.doc().validate().unwrap();

        assert_eq!(session.undo().as_deref(), Some("Import picture"));
        assert_eq!(names(&session), ["Spawn Area", "Paint"]);
        assert_eq!(session.doc().media_items().count(), 0);
        session.redo().unwrap();
        assert!(session.doc().layer(id).is_some());
        assert_eq!(session.doc().media_items().count(), 1);
    }

    #[test]
    fn a_second_import_of_the_same_file_gets_a_name_of_its_own() {
        let mut session = Session::new(Document::new());
        session.add_media_layer(png()).unwrap();
        session.add_media_layer(png()).unwrap();
        assert_eq!(
            names(&session),
            ["Spawn Area", "Paint", "Sky at dusk", "Sky at dusk 2"]
        );
    }

    #[test]
    fn an_import_goes_into_the_open_group_that_is_current() {
        let mut session = Session::new(Document::new());
        let group = session
            .exec_json(&serde_json::json!({"cmd": "add_layer", "kind": "group"}))
            .unwrap()["layer"]
            .as_u64()
            .map(LayerId)
            .unwrap();
        let id = session
            .add_media_layer(NewMedia {
                kind: MediaLayerKind::Sound {
                    kind: SoundKind::Positional,
                },
                file_name: "steps.wav".into(),
                source_path: String::new(),
                data: MediaData::Encoded {
                    format: "wav".into(),
                    bytes: Arc::from(&[0u8][..]),
                },
            })
            .unwrap();
        assert_eq!(session.doc().parent_of(id), Some(group));
        let LayerKind::Sound(sound) = &session.doc().layer(id).unwrap().kind else {
            panic!("not a sound")
        };
        assert_eq!(sound.kind, SoundKind::Positional);
        assert_eq!(sound.gain, 1.0);
        assert_eq!(session.history().undo_label(), Some("Import sound"));
    }

    #[test]
    fn a_new_layer_starts_at_the_playhead_when_that_option_is_on() {
        let mut session = Session::new(Document::new());
        session.options.create_layers_at_playhead = true;
        let second = crate::time::from_seconds(1.0);
        session.playback.seek(second);
        let id = session
            .add_media_layer(NewMedia {
                kind: MediaLayerKind::Model,
                file_name: "rock.obj".into(),
                source_path: String::new(),
                data: MediaData::Mesh(Arc::new(MeshData::default())),
            })
            .unwrap();
        let layer = session.doc().layer(id).unwrap();
        assert_eq!(layer.name, "rock");
        assert_eq!(layer.anim.visibility.keys()[0].time, second);
        assert_eq!(session.history().undo_label(), Some("Import model"));
    }

    #[test]
    fn content_of_the_wrong_kind_is_refused_and_changes_nothing() {
        let mut session = Session::new(Document::new());
        let mut wrong = png();
        wrong.kind = MediaLayerKind::Model;
        assert!(session.add_media_layer(wrong).is_err());
        let mut unknown = png();
        unknown.data = MediaData::Encoded {
            format: "tiff".into(),
            bytes: Arc::from(&[0u8][..]),
        };
        assert!(session.add_media_layer(unknown).is_err());
        assert_eq!(names(&session), ["Spawn Area", "Paint"]);
        assert!(!session.is_dirty());
        assert!(!session.history().can_undo());
    }

    #[test]
    fn layer_names_come_from_the_file_name() {
        assert_eq!(layer_name("rock.obj"), "rock");
        assert_eq!(layer_name("C:\\art\\old.rock.OBJ"), "old.rock");
        assert_eq!(layer_name("/art/noext"), "noext");
        assert_eq!(layer_name(".hidden"), ".hidden");
        assert_eq!(layer_name(""), "Layer");
    }
}
