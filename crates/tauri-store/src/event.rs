use crate::error::Result;
use crate::store::{Store, StoreId, StoreOptions, StoreState};
use crate::CollectionMarker;
use serde::Serialize;
use tauri::{AppHandle, Emitter as _, EventTarget, Runtime, WebviewWindow, Window};

pub const STORE_CONFIG_CHANGE_EVENT: &str = "tauri-store://config-change";

/// Owned config payload for deferred emit (avoids holding store lock during emit).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigPayloadOwned {
  id: StoreId,
  config: StoreOptions,
}
pub const STORE_STATE_CHANGE_EVENT: &str = "tauri-store://state-change";
pub const STORE_UNLOAD_EVENT: &str = "tauri-store://unload";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatePayload<'a> {
  id: &'a StoreId,
  state: &'a StoreState,
}

impl<'a, R, C> From<&'a Store<R, C>> for StatePayload<'a>
where
  R: Runtime,
  C: CollectionMarker,
{
  fn from(store: &'a Store<R, C>) -> Self {
    Self {
      id: &store.id,
      state: store.raw_state(),
    }
  }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigPayload<'a> {
  id: &'a StoreId,
  config: StoreOptions,
}

impl<'a, R, C> From<&'a Store<R, C>> for ConfigPayload<'a>
where
  R: Runtime,
  C: CollectionMarker,
{
  fn from(store: &'a Store<R, C>) -> Self {
    Self { id: &store.id, config: store.into() }
  }
}

pub(crate) fn emit<R, T, S>(app: &AppHandle<R>, event: &str, payload: &T, source: S) -> Result<()>
where
  R: Runtime,
  T: Serialize + ?Sized,
  S: Into<EventSource>,
{
  let source: EventSource = source.into();
  if let Some(source) = source.0 {
    emit_filter(app, event, payload, |it| it != source)
  } else {
    emit_all(app, event, payload)
  }
}

fn emit_all<R, T>(app: &AppHandle<R>, event: &str, payload: &T) -> Result<()>
where
  R: Runtime,
  T: Serialize + ?Sized,
{
  app.emit_filter(event, payload, |target| {
    matches!(target, EventTarget::WebviewWindow { .. })
  })?;
  Ok(())
}

fn emit_filter<R, T, F>(app: &AppHandle<R>, event: &str, payload: &T, f: F) -> Result<()>
where
  R: Runtime,
  T: Serialize + ?Sized,
  F: Fn(&str) -> bool,
{
  #[rustfmt::skip]
  app.emit_filter(event, payload, |target| {
    matches!(target, EventTarget::WebviewWindow { label } if f(label))
  })?;
  Ok(())
}

/// Source of a store event.
pub struct EventSource(Option<String>);

impl EventSource {
  #[inline]
  pub const fn is_backend(&self) -> bool {
    self.0.is_none()
  }
}

impl From<&str> for EventSource {
  fn from(source: &str) -> Self {
    Self(Some(String::from(source)))
  }
}

impl From<Option<&str>> for EventSource {
  fn from(source: Option<&str>) -> Self {
    Self(source.map(String::from))
  }
}

impl From<String> for EventSource {
  fn from(source: String) -> Self {
    Self(Some(source))
  }
}

impl From<&String> for EventSource {
  fn from(source: &String) -> Self {
    Self(Some(source.to_owned()))
  }
}

impl From<Option<String>> for EventSource {
  fn from(source: Option<String>) -> Self {
    Self(source)
  }
}

/// Schedules config-change emit on the main thread to avoid deadlock when the store lock
/// is held during window creation (`emit_filter` blocks on window manager).
pub(crate) fn emit_config_change_deferred<R, S>(
  app: &AppHandle<R>,
  id: StoreId,
  config: StoreOptions,
  source: S,
) -> Result<()>
where
  R: Runtime,
  S: Into<EventSource>,
{
  let app_for_closure = app.clone();
  let source: EventSource = source.into();
  let payload = ConfigPayloadOwned { id, config };
  app.run_on_main_thread(move || {
    let _ = emit(
      &app_for_closure,
      STORE_CONFIG_CHANGE_EVENT,
      &payload,
      source,
    );
  })?;
  Ok(())
}

impl From<&WebviewWindow> for EventSource {
  fn from(window: &WebviewWindow) -> Self {
    Self(Some(window.label().to_owned()))
  }
}

impl From<&Window> for EventSource {
  fn from(window: &Window) -> Self {
    Self(Some(window.label().to_owned()))
  }
}
