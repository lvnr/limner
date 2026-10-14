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
    /// Brings another document in as a group after the current layer, as one undo
    /// step: everything in it, with its animation. The group becomes the current layer.
    pub fn import_document(
        &mut self,
        other: &crate::model::Document,
        name: &str,
    ) -> Result<LayerId, CommandError> {
        self.edit("Import project", |session| {
            let (parent, index) = session.default_insertion_point();
            let group = layers::graft_document(session.doc_mut(), other, parent, index, name)?;
            // It arrives folded: one row in the layer list, and the next thing added
            // goes after it, not into it.
            if let Some(layer) = session.doc_mut().layer_mut(group) {
                layer.collapsed = true;
            }
            session.set_current_layer(group);
            Ok(group)
        })
    }

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

#[cfg(test)]
mod import_tests {
    use crate::model::{Document, Interp, LayerKind, MediaData, MediaKind, Picture, Projection};
    use crate::session::Session;
    use crate::time::from_seconds;
    use std::sync::Arc;

    /// A document with a keyed group holding a paint layer of two frames that share
    /// nothing with any other document, and a picture.
    fn other() -> Document {
        let mut doc = Document::new();
        let root = doc.root();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        let group = doc.new_layer("Arm", LayerKind::Group(Default::default()), 0);
        let group = doc.insert_layer(group, root, usize::MAX).unwrap();
        doc.move_layer(paint, group, 0).unwrap();
        doc.layer_mut(group)
            .unwrap()
            .anim
            .opacity
            .set(from_seconds(1.0), 0.5, Interp::Linear);
        let second = doc.add_drawing(Default::default());
        let frames = doc.layer_mut(paint).unwrap().as_paint_mut().unwrap();
        frames.drawings.push(second);
        frames.frames = vec![0, 1, 0];
        let image = doc.add_media(
            "photo.png",
            MediaKind::Image,
            MediaData::RawImage {
                width: 1,
                height: 1,
                channels: 3,
                pixels: Arc::from(vec![9u8, 9, 9]),
            },
        );
        let picture = doc.new_layer(
            "Photo",
            LayerKind::Picture(Picture {
                media: Some(image),
                projection: Projection::Flat,
                viewer_locked: false,
                source_path: String::new(),
            }),
            0,
        );
        doc.insert_layer(picture, root, usize::MAX).unwrap();
        doc.validate().unwrap();
        doc
    }

    #[test]
    fn another_project_comes_in_as_a_group_with_everything_in_it() {
        let mut session = Session::new(Document::new());
        let before = session.doc().layer_count();
        let group = session.import_document(&other(), "Robot").unwrap();
        let doc = session.doc();
        doc.validate().unwrap();
        assert_eq!(doc.layer(group).unwrap().name, "Robot");
        assert!(doc.layer(group).unwrap().collapsed, "folded, as one row");
        assert_eq!(session.current_layer(), Some(group));
        // Spawn area, the arm with its paint layer, and the photo.
        assert_eq!(doc.layer_count(), before + 5);
        let arm = doc
            .find_by_path("Root/Robot/Arm")
            .expect("the group came along");
        assert_eq!(
            doc.layer(arm).unwrap().anim.opacity.len(),
            1,
            "with its keys"
        );
        let paint = doc.find_by_path("Root/Robot/Arm/Paint").unwrap();
        let frames = doc.layer(paint).unwrap().as_paint().unwrap();
        assert_eq!(frames.frames, [0, 1, 0], "and its frames");
        assert!(frames.drawings.iter().all(|d| doc.drawing(*d).is_some()));
        let own = doc.find_by_path("Root/Paint").unwrap();
        let own_drawing = doc.layer(own).unwrap().as_paint().unwrap().drawings[0];
        assert!(
            !frames.drawings.contains(&own_drawing),
            "under ids of its own"
        );
        let photo = doc.find_by_path("Root/Robot/Photo").unwrap();
        let LayerKind::Picture(picture) = &doc.layer(photo).unwrap().kind else {
            panic!("not a picture");
        };
        assert!(
            doc.media(picture.media.unwrap()).is_some(),
            "the picture's data too"
        );
        assert_eq!(
            doc.settings.default_viewpoint,
            Document::new().settings.default_viewpoint
        );

        assert_eq!(session.history().undo_label(), Some("Import project"));
        session.undo();
        assert_eq!(session.doc().layer_count(), before);
        // Twice in, the second gets a name of its own.
        session.import_document(&other(), "Robot").unwrap();
        let again = session.import_document(&other(), "Robot").unwrap();
        assert_eq!(session.doc().layer(again).unwrap().name, "Robot 2");
        assert_eq!(
            session.doc().parent_of(again),
            Some(session.doc().root()),
            "beside the first, not inside it"
        );
    }
}
