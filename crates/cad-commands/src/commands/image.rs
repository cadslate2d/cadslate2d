//! IMAGEATTACH, IMAGE, IMAGEADJUST, and IMAGEFRAME (ADR-0042 §6, #525).
//!
//! A picture comes in only through the host (§3), which makes it — at
//! most 2,048 pixels on its long side, a JPEG or an opaque PNG — and hands
//! the bytes to the waiting command as `CommandInput::Picture`.
//! `IMAGEATTACH` (`IAT`) then places it by an insertion point, its width
//! across in drawing units, and a rotation, the picture added with its
//! image in one transaction; the same picture attached again is the one
//! the drawing holds (`TransactionBuilder::picture`). `IMAGE` (`IM`)
//! lists the drawing's pictures, `Detach`es one with its images, and
//! `Reload`s one from a file, its images keeping their frames.
//! `IMAGEADJUST` (`IAD`) fades the images selected, `IMAGEFRAME` shows or
//! hides every image's frame. One undo each.

use std::collections::HashSet;

use cad_core::{EntityId, Point2, Vec2};
use cad_document::{Entity, ImageDef, ImageDefId, ImageEntity, ImageFormat, MOST_FADE};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, HostRequest, OverlayItem, Prompt, Step};

/// What the preview of a picture not yet in the drawing shows: no
/// definition the adapter has, so the frame filled in grey.
const NOT_YET: ImageDefId = ImageDefId::from_raw(u32::MAX);

/// "1,200 × 800 pixels".
fn size_of(def: &ImageDef) -> String {
    format!("{} × {} pixels", def.width, def.height)
}

/// The image of `picture`, `width` across, turned `degrees` about its
/// lower-left corner at `insertion`: square pixels.
fn laid(
    def: ImageDefId,
    picture: &ImageDef,
    insertion: Point2,
    width: f64,
    degrees: f64,
) -> Option<ImageEntity> {
    let size = width / f64::from(picture.width);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let u = Vec2::new(cos, sin) * size;
    let v = Vec2::new(-sin, cos) * size;
    ImageEntity::new(def, insertion, u, v, [picture.width, picture.height]).ok()
}

/// The definition the drawing holds of the same picture, if it does.
fn held(ctx: &CommandContext<'_>, picture: &ImageDef) -> Option<ImageDefId> {
    ctx.drawing
        .image_defs()
        .iter()
        .find(|(_, mine)| mine.digest == picture.digest && mine.bytes == picture.bytes)
        .map(|(id, _)| id)
}

// ── IMAGEATTACH ──

#[derive(Debug)]
enum AttachState {
    /// Waiting for the host's picture.
    File,
    Point,
    Width(Point2),
    Rotation(Point2, f64),
}

/// `IMAGEATTACH` (`IAT`): a picture from the host, placed.
#[derive(Debug)]
pub(crate) struct ImageAttachCommand {
    state: AttachState,
    picture: Option<ImageDef>,
    /// What the preview shows: the drawing's own definition of the
    /// picture when it has one, so the picture itself; else none.
    shown: ImageDefId,
}

impl ImageAttachCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: AttachState::File,
            picture: None,
            shown: NOT_YET,
        }
    }

    /// The width it takes on Enter: one drawing unit a pixel.
    fn default_width(picture: &ImageDef) -> f64 {
        f64::from(picture.width)
    }

    /// Adds the picture, or finds it held, with its image, as one change.
    fn attach(&self, ctx: &mut CommandContext<'_>, insertion: Point2, width: f64, degrees: f64) {
        let Some(picture) = self.picture.clone() else {
            return;
        };
        let mut transaction = ctx.transaction();
        let attached = transaction.picture(picture.clone()).and_then(|def| {
            let image = laid(def, &picture, insertion, width, degrees).ok_or(
                cad_document::TransactionError::BadImage(cad_document::ImageError::NoArea),
            )?;
            transaction.insert(Entity::Image(image))?;
            Ok(def)
        });
        match attached {
            Ok(def) => {
                transaction.commit();
                let name = ctx
                    .drawing
                    .image_defs()
                    .get(def)
                    .map_or_else(String::new, |def| def.name.clone());
                ctx.message(format!(
                    "Attached \"{name}\", {}, {} across.",
                    size_of(&picture),
                    crate::summary::number(width)
                ));
            }
            Err(error) => {
                drop(transaction);
                ctx.message(crate::log::sentence(&format!("No image: {error}")));
            }
        }
    }
}

impl Command for ImageAttachCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::ImageFile);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match (&self.state, &self.picture) {
            (AttachState::File, _) | (_, None) => Prompt::new("Choose the picture to attach"),
            (AttachState::Point, _) => Prompt::new("Specify insertion point"),
            (AttachState::Width(_), Some(picture)) => Prompt::new("Specify width")
                .with_default(crate::summary::number(Self::default_width(picture))),
            (AttachState::Rotation(..), _) => {
                Prompt::new("Specify rotation angle").with_default("0")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            AttachState::File => ExpectedInput::new(InputKind::Nothing),
            AttachState::Point => ExpectedInput::point(),
            AttachState::Width(_) => ExpectedInput::distance(),
            AttachState::Rotation(..) => ExpectedInput::angle(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, AttachState::File);
        self.state = match (state, input) {
            (AttachState::File, CommandInput::Picture(picture)) => {
                self.shown = held(ctx, &picture).unwrap_or(NOT_YET);
                ctx.message(format!(
                    "\"{}\": {}, {}.",
                    picture.name,
                    size_of(&picture),
                    match picture.format {
                        ImageFormat::Png => "PNG",
                        ImageFormat::Jpeg => "JPEG",
                    }
                ));
                self.picture = Some(picture);
                AttachState::Point
            }
            (AttachState::File, CommandInput::Accept) => return Step::Finished,
            (AttachState::Point, CommandInput::Point(insertion)) => AttachState::Width(insertion),
            (AttachState::Point, CommandInput::Accept) => return Step::Finished,
            (AttachState::Width(insertion), CommandInput::Distance(width)) => {
                if width.is_finite() && width > 0.0 {
                    AttachState::Rotation(insertion, width)
                } else {
                    ctx.message("The width must be more than zero.");
                    AttachState::Width(insertion)
                }
            }
            (AttachState::Width(insertion), CommandInput::Accept) => {
                let width = self.picture.as_ref().map_or(1.0, Self::default_width);
                AttachState::Rotation(insertion, width)
            }
            (AttachState::Rotation(insertion, width), CommandInput::Angle(degrees)) => {
                self.attach(ctx, insertion, width, degrees);
                return Step::Finished;
            }
            (AttachState::Rotation(insertion, width), CommandInput::Accept) => {
                self.attach(ctx, insertion, width, 0.0);
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }

    /// The image where the next input would put it: at the pointer, as
    /// wide as the pointer is far, turned toward it.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let Some(picture) = &self.picture else {
            return;
        };
        let (insertion, width, degrees) = match self.state {
            AttachState::File => return,
            AttachState::Point => (cursor, Self::default_width(picture), 0.0),
            AttachState::Width(insertion) => (insertion, (cursor - insertion).length(), 0.0),
            AttachState::Rotation(insertion, width) => (
                insertion,
                width,
                crate::command::angle_degrees(cursor - insertion),
            ),
        };
        if let Some(image) = laid(self.shown, picture, insertion, width, degrees) {
            out.push(OverlayItem::PreviewEntity(Entity::Image(image)));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            AttachState::Width(insertion) | AttachState::Rotation(insertion, _) => Some(insertion),
            _ => None,
        }
    }
}

// ── IMAGE ──

const LIST: Keyword = Keyword::new("?");
const DETACH: Keyword = Keyword::new("Detach");
const RELOAD: Keyword = Keyword::new("Reload");
const OPTIONS: [Keyword; 3] = [LIST, DETACH, RELOAD];

#[derive(Debug)]
enum ImageState {
    Option,
    /// The name of the picture to detach (true) or reload.
    Name(bool),
    /// Waiting for the host's picture to reload this one with.
    File(ImageDefId),
}

/// `IMAGE` (`IM`): the drawing's pictures listed, detached, or reloaded.
#[derive(Debug)]
pub(crate) struct ImageCommand {
    state: ImageState,
}

impl ImageCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: ImageState::Option,
        }
    }

    /// Each picture: its name, its size, how many images show it.
    fn list(ctx: &mut CommandContext<'_>) {
        let drawing = &*ctx.drawing;
        if drawing.image_defs().is_empty() {
            ctx.message("The drawing has no images.");
            return;
        }
        let lines: Vec<String> = drawing
            .image_defs()
            .iter()
            .map(|(id, def)| {
                let shown = drawing
                    .records()
                    .filter(|(_, record)| {
                        matches!(&*record.entity, Entity::Image(image) if image.def == id)
                    })
                    .count();
                let held = if def.is_missing() {
                    "missing: reload it".to_owned()
                } else {
                    let kind = match def.format {
                        ImageFormat::Png => "PNG",
                        ImageFormat::Jpeg => "JPEG",
                    };
                    format!("{kind}, {} KB", def.bytes.len().div_ceil(1024))
                };
                format!(
                    "\"{}\": {}, {held}, shown by {}.",
                    def.name,
                    size_of(def),
                    counted(shown, "image")
                )
            })
            .collect();
        for line in lines {
            ctx.message(line);
        }
    }

    fn find(ctx: &mut CommandContext<'_>, name: &str) -> Option<ImageDefId> {
        let found = ctx.drawing.image_defs().find(name.trim());
        if found.is_none() {
            ctx.message(format!("There is no image \"{}\".", name.trim()));
        }
        found
    }
}

impl Command for ImageCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            ImageState::Option => Prompt::new("Enter image option").with_default("?"),
            ImageState::Name(true) => Prompt::new("Enter image name to detach"),
            ImageState::Name(false) => Prompt::new("Enter image name to reload"),
            ImageState::File(_) => Prompt::new("Choose the picture to reload it with"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            ImageState::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            ImageState::Name(_) => ExpectedInput::text(),
            ImageState::File(_) => ExpectedInput::new(InputKind::Nothing),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, ImageState::Option);
        self.state = match (state, input) {
            (ImageState::Option, CommandInput::Keyword(DETACH)) => ImageState::Name(true),
            (ImageState::Option, CommandInput::Keyword(RELOAD)) => ImageState::Name(false),
            (ImageState::Option, CommandInput::Keyword(_) | CommandInput::Accept) => {
                Self::list(ctx);
                return Step::Finished;
            }
            (ImageState::Name(_), CommandInput::Accept) => return Step::Finished,
            (ImageState::Name(true), CommandInput::Text(name)) => {
                let Some(id) = Self::find(ctx, &name) else {
                    return Step::Continue;
                };
                let name = ctx.drawing.image_defs().get(id).map(|d| d.name.clone());
                let mut transaction = ctx.transaction();
                match transaction.detach_image_def(id) {
                    Ok(images) => {
                        transaction.commit();
                        ctx.message(format!(
                            "Detached \"{}\" and {}.",
                            name.unwrap_or_default(),
                            counted(images, "image")
                        ));
                    }
                    Err(cad_document::TransactionError::ImageDefInUse(_)) => {
                        drop(transaction);
                        ctx.message("A block holds an image of it: it stays.");
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(crate::log::sentence(&error.to_string()));
                    }
                }
                return Step::Finished;
            }
            (ImageState::Name(false), CommandInput::Text(name)) => match Self::find(ctx, &name) {
                Some(id) => {
                    ctx.request_host(HostRequest::ImageFile);
                    ImageState::File(id)
                }
                None => ImageState::Name(false),
            },
            (ImageState::File(id), CommandInput::Picture(picture)) => {
                let size = size_of(&picture);
                let mut transaction = ctx.transaction();
                match transaction.reload_image_def(id, picture) {
                    Ok(()) => {
                        transaction.commit();
                        let name = ctx.drawing.image_defs().get(id).map(|d| d.name.clone());
                        ctx.message(format!(
                            "Reloaded \"{}\": {size}.",
                            name.unwrap_or_default()
                        ));
                    }
                    Err(cad_document::TransactionError::ImageSizeMismatch(_)) => {
                        drop(transaction);
                        ctx.message(
                            "A block holds an image of it: reload it with a picture of the same size.",
                        );
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(crate::log::sentence(&error.to_string()));
                    }
                }
                return Step::Finished;
            }
            (ImageState::File(_), CommandInput::Accept) => return Step::Finished,
            (state, _) => state,
        };
        Step::Continue
    }
}

// ── IMAGEADJUST ──

#[derive(Debug)]
enum AdjustState {
    Select,
    /// The images, and the fade they share, if they do.
    Fade(Vec<EntityId>, Option<u8>),
}

/// `IMAGEADJUST` (`IAD`): the fade of the images selected.
#[derive(Debug)]
pub(crate) struct ImageAdjustCommand {
    state: AdjustState,
}

impl ImageAdjustCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: AdjustState::Select,
        }
    }

    /// The images of the selection, which it leaves; the rest said.
    fn chosen(ctx: &mut CommandContext<'_>) -> Option<AdjustState> {
        let selected: HashSet<EntityId> = ctx.selection.iter().collect();
        if selected.is_empty() {
            return None;
        }
        ctx.selection.clear();
        let mut images: Vec<EntityId> = selected
            .iter()
            .copied()
            .filter(|id| matches!(ctx.drawing.entity(*id), Some(Entity::Image(_))))
            .collect();
        images.sort_unstable();
        let others = selected.len() - images.len();
        if others > 0 {
            ctx.message(format!(
                "{} not an image.",
                if others == 1 {
                    "1 object is".to_owned()
                } else {
                    format!("{others} objects are")
                }
            ));
        }
        if images.is_empty() {
            ctx.message("No images selected.");
            return Some(AdjustState::Select);
        }
        let fades: HashSet<u8> = images
            .iter()
            .filter_map(|id| match ctx.drawing.entity(*id) {
                Some(Entity::Image(image)) => Some(image.fade),
                _ => None,
            })
            .collect();
        let shared = (fades.len() == 1)
            .then(|| fades.into_iter().next())
            .flatten();
        Some(AdjustState::Fade(images, shared))
    }
}

impl Command for ImageAdjustCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if !ctx.selection.is_empty() {
            match Self::chosen(ctx) {
                Some(AdjustState::Select) => return Step::Finished,
                Some(state) => self.state = state,
                None => {}
            }
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            AdjustState::Select => Prompt::new(SELECT_OBJECTS),
            AdjustState::Fade(_, shared) => {
                let prompt = Prompt::new(format!("Enter fade value (0-{MOST_FADE})"));
                match shared {
                    Some(fade) => prompt.with_default(fade.to_string()),
                    None => prompt,
                }
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            AdjustState::Select => selection_expected(),
            AdjustState::Fade(..) => ExpectedInput::factor(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, AdjustState::Select);
        self.state = match (state, input) {
            (AdjustState::Select, CommandInput::Accept) => match Self::chosen(ctx) {
                None => {
                    ctx.message(NOTHING_SELECTED);
                    return Step::Finished;
                }
                Some(AdjustState::Select) => return Step::Finished,
                Some(next) => next,
            },
            (AdjustState::Fade(images, shared), CommandInput::Distance(value)) => {
                if !(0.0..=f64::from(MOST_FADE)).contains(&value) || value.fract() != 0.0 {
                    ctx.message(format!("The fade is a whole number from 0 to {MOST_FADE}."));
                    self.state = AdjustState::Fade(images, shared);
                    return Step::Continue;
                }
                let fade = value as u8;
                let faded: Vec<(EntityId, ImageEntity)> = images
                    .iter()
                    .filter_map(|id| match ctx.drawing.entity(*id) {
                        Some(Entity::Image(image)) => Some((
                            *id,
                            ImageEntity {
                                fade,
                                ..image.clone()
                            },
                        )),
                        _ => None,
                    })
                    .collect();
                let mut transaction = ctx.transaction();
                for (id, image) in &faded {
                    transaction
                        .replace(*id, Entity::Image(image.clone()))
                        .expect("an image of the drawing, faded within bounds");
                }
                transaction.commit();
                ctx.message(format!(
                    "{} faded to {fade}.",
                    counted(faded.len(), "image")
                ));
                return Step::Finished;
            }
            (AdjustState::Fade(..), CommandInput::Accept) => return Step::Finished,
            (state, _) => state,
        };
        Step::Continue
    }
}

// ── IMAGEFRAME ──

/// `IMAGEFRAME`: every image's frame shown (1) or hidden (0), a drawing
/// setting, one undo.
#[derive(Debug, Default)]
pub(crate) struct ImageFrameCommand {
    /// Whether frames are shown now: the default.
    shown: bool,
}

impl Command for ImageFrameCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.shown = ctx.drawing.settings().image_frame;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter image frame setting (0 or 1)").with_default(if self.shown {
            "1"
        } else {
            "0"
        })
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::factor()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let shown = match input {
            // AutoCAD's two answers, and nothing between.
            CommandInput::Distance(value) if [0.0, 1.0].contains(&value) => value > 0.5,
            CommandInput::Accept => return Step::Finished,
            _ => {
                ctx.message("Enter 0 to hide image frames or 1 to show them.");
                return Step::Continue;
            }
        };
        let mut transaction = ctx.transaction();
        transaction.set_image_frame(shown);
        transaction.commit();
        ctx.message(if shown {
            "Image frames are shown."
        } else {
            "Image frames are hidden."
        });
        Step::Finished
    }
}
