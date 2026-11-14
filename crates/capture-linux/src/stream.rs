//! The PipeWire stream of the portal's session, read on its own thread: shared memory buffers of BGRA rows,
//! the cursor as metadata.

use std::io::Cursor;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use sc_ohseearr_capture::{Pixels, unpad};
use spa::buffer::meta::MetaCursor;
use spa::buffer::{ChunkFlags, DataType};
use spa::param::ParamType;
use spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use spa::param::video::{VideoFormat, VideoInfoRaw};
use spa::pod::serialize::PodSerializer;
use spa::pod::{ChoiceValue, Object, Pod, Property, Value};
use spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};

use crate::FRAME_RATE;
use crate::capture::{Shared, Shot, Want};
use crate::still::Still;

/// How often the loop looks whether it should end
const TICK: Duration = Duration::from_millis(100);

/// What the stream's callbacks keep.
struct Local {
    shared: Arc<Shared>,
    /// The settled format (BGRx or BGRA)
    format: Option<VideoInfoRaw>,
    still: Still,
}

/// Reads the stream of `node` on the PipeWire `remote` until `shared.stop`: hands an image to a waiting grab
/// (`State::want`) and reports the stream's end (`Shared::end`).
pub(crate) fn run(node: u32, remote: OwnedFd, shared: &Arc<Shared>) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopBox::new(None)?;
    let context = pw::context::ContextBox::new(mainloop.loop_(), None)?;
    let core = context.connect_fd(remote, None)?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "sc-ohseearr-capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;
    let _listener = stream
        .add_local_listener_with_user_data(Local {
            shared: shared.clone(),
            format: None,
            still: Still::new(Instant::now()),
        })
        .state_changed(|_, local, old, new| {
            log::debug!("screen capture: stream {old:?} -> {new:?}");
            match new {
                pw::stream::StreamState::Error(e) => local.shared.end(format!("the stream failed: {e}")),
                // The window was closed, the session revoked
                pw::stream::StreamState::Unconnected => local.shared.end("the stream was closed"),
                _ => {}
            }
        })
        .param_changed(on_param)
        .process(on_process)
        .register()?;

    // Packed 8 bit BGR only (`Pixels::Bgra`), and no modifier: the buffers come as shared memory, not as
    // dmabufs
    let mut format = spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
        ),
        spa::pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle { width: 1920, height: 1080 },
            Rectangle { width: 1, height: 1 },
            Rectangle { width: 16384, height: 16384 }
        ),
        spa::pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: 30, denom: 1 },
            Fraction { num: 0, denom: 1 },
            Fraction { num: 1000, denom: 1 }
        ),
    );
    // A fixed value, not a range: the compositor's own maximum (the monitor's rate) must not win
    format.properties.push(Property::new(
        FormatProperties::VideoMaxFramerate.as_raw(),
        Value::Fraction(Fraction { num: FRAME_RATE, denom: 1 }),
    ));
    let format = pod_bytes(format);
    let mut params = [Pod::from_bytes(&format).expect("a serialized pod")];
    stream.connect(
        spa::utils::Direction::Input,
        Some(node),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    while !shared.stop.load(Ordering::Relaxed) {
        mainloop.loop_().iterate(pw::loop_::Timeout::Finite(TICK));
    }
    let _ = stream.disconnect();
    Ok(())
}

/// The format is settled: asks for shared memory buffers and the cursor metadata.
fn on_param(stream: &pw::stream::Stream, local: &mut Local, id: u32, param: Option<&Pod>) {
    let Some(param) = param else { return };
    if id != ParamType::Format.as_raw() {
        return;
    }
    let mut info = VideoInfoRaw::default();
    let raw_video = matches!(spa::param::format_utils::parse_format(param), Ok((MediaType::Video, MediaSubtype::Raw)));
    if !raw_video || info.parse(param).is_err() {
        return local.shared.end("the stream's format is not raw video");
    }
    if !matches!(info.format(), VideoFormat::BGRx | VideoFormat::BGRA) {
        return local.shared.end(format!("pixel format {:?} not supported", info.format()));
    }
    log::debug!(
        "screen capture: {:?}, {}×{}, at most {}/{} images per second",
        info.format(),
        info.size().width,
        info.size().height,
        info.max_framerate().num,
        info.max_framerate().denom
    );
    local.format = Some(info);

    let memory = (1 << DataType::MemFd.as_raw()) | (1 << DataType::MemPtr.as_raw());
    let buffers = pod_bytes(Object {
        type_: SpaTypes::ObjectParamBuffers.as_raw(),
        id: ParamType::Buffers.as_raw(),
        properties: vec![Property::new(
            spa::sys::SPA_PARAM_BUFFERS_dataType,
            Value::Choice(ChoiceValue::Int(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Flags { default: memory, flags: vec![] },
            ))),
        )],
    });
    // Room for the cursor's bitmap too, as the compositors expect it (OBS asks the same way); only the
    // position is read
    let cursor_size = |side: usize| {
        (size_of::<spa::sys::spa_meta_cursor>() + size_of::<spa::sys::spa_meta_bitmap>() + side * side * 4) as i32
    };
    let cursor = pod_bytes(Object {
        type_: SpaTypes::ObjectParamMeta.as_raw(),
        id: ParamType::Meta.as_raw(),
        properties: vec![
            Property::new(spa::sys::SPA_PARAM_META_type, Value::Id(Id(spa::sys::SPA_META_Cursor))),
            Property::new(
                spa::sys::SPA_PARAM_META_size,
                Value::Choice(ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range { default: cursor_size(64), min: cursor_size(1), max: cursor_size(1024) },
                ))),
            ),
        ],
    });
    let mut params =
        [Pod::from_bytes(&buffers).expect("a serialized pod"), Pod::from_bytes(&cursor).expect("a serialized pod")];
    if let Err(e) = stream.update_params(&mut params) {
        local.shared.end(format!("pipewire: update_params: {e}"));
    }
}

/// A buffer came: follows the cursor, and hands the image to a grab that waits for it.
fn on_process(stream: &pw::stream::Stream, local: &mut Local) {
    let Some(mut buffer) = stream.dequeue_buffer() else { return };
    let now = Instant::now();
    // Id 0: no cursor (hidden, or not in the window)
    let cursor = buffer.find_meta::<MetaCursor>().filter(|c| c.is_valid()).map(|c| (c.position().x, c.position().y));
    local.still.cursor(cursor, now);

    let Some(data) = buffer.datas_mut().first_mut() else { return };
    let (size, offset, stride, flags) =
        (data.chunk().size(), data.chunk().offset() as usize, data.chunk().stride(), data.chunk().flags());
    // No image: only the cursor changed (KWin marks such a buffer's chunk as corrupted)
    if size == 0 || flags.contains(ChunkFlags::CORRUPTED) {
        local.shared.state().skipped_pointer += 1;
        return;
    }
    let rested = local.still.image(now);
    let state = local.shared.state();
    match state.want {
        Some(Want::Next) => {}
        Some(Want::Rested) if rested.is_some() => {}
        _ => return,
    }
    let Some(info) = local.format else { return };
    let (width, height) = (info.size().width, info.size().height);
    let row = width as usize * 4;
    let bytes = data.data().and_then(|bytes| bytes.get(offset..));
    let stride = usize::try_from(stride).ok().filter(|stride| *stride >= row);
    let (Some(bytes), Some(stride)) = (bytes, stride) else {
        drop(state);
        return local.shared.end(format!("cannot read the stream's buffers ({:?}, stride {stride:?})", data.type_()));
    };
    if height == 0 || bytes.len() < stride * (height as usize - 1) + row {
        drop(state);
        return local.shared.end(format!("the stream's buffer is too small for {width}×{height}"));
    }
    let pixels = Pixels::Bgra { data: unpad(bytes, stride, width, height), width, height };
    // The cursor may hang over the window's edge
    let cursor = cursor
        .filter(|&(x, y)| x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height)
        .map(|(x, y)| (x as f32, y as f32));
    local.shared.deliver(state, Shot { pixels, cursor, rested: rested.unwrap_or_default() });
}

fn pod_bytes(object: Object) -> Vec<u8> {
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object)).expect("a pod in memory").0.into_inner()
}
