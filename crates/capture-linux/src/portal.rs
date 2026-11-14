//! The ScreenCast portal (xdg-desktop-portal): one window, the cursor as metadata, a restore token.

use std::os::fd::OwnedFd;

use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use ashpd::desktop::{PersistMode, ResponseError, Session};
use ashpd::zbus::block_on;

use crate::capture::OpenError;

/// A started session: its stream's PipeWire node on the PipeWire remote, and the token for the next session.
pub(crate) struct Opened {
    pub(crate) session: Session<Screencast>,
    pub(crate) node: u32,
    pub(crate) remote: OwnedFd,
    pub(crate) restore_token: Option<String>,
}

/// Asks the portal for one window. Blocks until it answered (while its dialog is open).
pub(crate) fn open(restore_token: Option<&str>) -> Result<Opened, OpenError> {
    block_on(async {
        let proxy = Screencast::new().await?;
        let session = proxy.create_session(Default::default()).await?;
        match start(&proxy, &session, restore_token).await {
            Ok((node, remote, restore_token)) => Ok(Opened { session, node, remote, restore_token }),
            Err(e) => {
                let _ = session.close().await;
                Err(e)
            }
        }
    })
    .map_err(|e| match e {
        ashpd::Error::Response(ResponseError::Cancelled) => OpenError::Cancelled,
        e => OpenError::Portal(e.to_string()),
    })
}

async fn start(
    proxy: &Screencast,
    session: &Session<Screencast>,
    restore_token: Option<&str>,
) -> ashpd::Result<(u32, OwnedFd, Option<String>)> {
    // A mode the portal does not offer closes the session. Without the metadata there is no cursor: one drawn
    // into the image would lie over what is read
    let cursor_mode = if proxy.available_cursor_modes().await?.contains(CursorMode::METADATA) {
        CursorMode::METADATA
    } else {
        log::warn!("screen capture: the portal offers no cursor metadata, frames come without the cursor");
        CursorMode::HIDDEN
    };
    proxy
        .select_sources(
            session,
            SelectSourcesOptions::default()
                .set_cursor_mode(cursor_mode)
                .set_sources(SourceType::WINDOW)
                .set_multiple(false)
                .set_restore_token(restore_token)
                // Until the user revokes it: the token outlives this process
                .set_persist_mode(PersistMode::ExplicitlyRevoked),
        )
        .await?
        .response()?;
    let response = proxy.start(session, None, Default::default()).await?.response()?;
    let Some(stream) = response.streams().first() else {
        return Err(ashpd::Error::NoResponse);
    };
    let node = stream.pipe_wire_node_id();
    let restore_token = response.restore_token().map(str::to_owned);
    let remote = proxy.open_pipe_wire_remote(session, Default::default()).await?;
    Ok((node, remote, restore_token))
}

/// Closes the session: the compositor stops the stream.
pub(crate) fn close(session: &Session<Screencast>) {
    if let Err(e) = block_on(session.close()) {
        log::debug!("screen capture: closing the portal's session: {e}");
    }
}
