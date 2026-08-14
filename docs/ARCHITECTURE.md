# cosmolith architecture

Status: living document, first version written 2026-08-14. Reflects the
source tree at commit `d61a617` on branch
`rahul/generated-config-persistence-20260814`.

## 1. What cosmolith is

cosmolith is a small standalone Rust daemon (`src/main.rs`) that bridges
COSMIC's own configuration store (`cosmic-config`, backed by the
`com.system76.CosmicComp` and shortcuts namespaces) to whatever Wayland
compositor is actually running the session. In the Regolith COSMIC
integration, that compositor is Sway, not `cosmic-comp`: Regolith runs
COSMIC's shell components (panel, applets, settings, launcher, etc.) on top
of Sway rather than the native COSMIC compositor. COSMIC's own settings
UI (`cosmic-settings`) writes to `cosmic-config`, but Sway has no idea that
store exists and won't pick up changes made there on its own. cosmolith is
the process that watches `cosmic-config` for changes and translates them
into the equivalent Sway IPC commands (and, as of last night's work, also
persists a Sway-config-file copy of those commands so they survive without
cosmolith running).

It also has scaffolding for other backends (Hyprland, GNOME, KDE), but only
the Sway backend is exercised by the Regolith COSMIC session; the others are
present as `Compositor` trait implementations with unimplemented gaps
noted inline (see section 5).

This doc does not restate the wider Regolith/COSMIC proposal-completion
status; see the vault
(`/Users/rahul/My_vault/GSOC 2026/ccextractor/03_Regolith_COSMIC_Session/`)
for that. This is scoped to cosmolith the binary/crate only.

## 2. Architecture

### 2.1 Entry point and main loop (`src/main.rs`, `src/lib.rs`)

`main.rs` wires everything together:

1. Opens a `cosmic_config::Config` handle for `com.system76.CosmicComp`
   (mostly to fail fast if the config store isn't reachable).
2. Creates an `mpsc::channel<Event>` shared (via `Arc<Mutex<Sender<Event>>>`)
   between the watchers and the main loop.
3. Starts the input watcher (`watcher::input::start_input_watcher`) and the
   shortcuts watcher (`watcher::shortcuts::start_shortcuts_watcher`), then
   sends the current keyboard config as an initial burst of events
   (`send_initial_input_events`) so a freshly started cosmolith applies
   whatever xkb layout/variant is already configured.
4. Detects the current desktop session (`identifier::get_current_session`)
   and initializes the matching `Compositor` impl
   (`compositor::init_compositor`).
5. Calls `compositor.replay_persisted_config()` once at startup — this is
   the new persistence-replay step, see section 3.
6. Runs a blocking loop on `rx.recv_timeout(5s)`: each received `Event` is
   handed to `compositor.apply_event(event)`; the 5s timeout is just a
   heartbeat so the loop stays responsive to Ctrl+C, not a poll interval.

`lib.rs` re-exports `error`, `event`, `watcher`, `compositor`, `identifier`,
and `persistence` as a library target, which is what lets the `#[cfg(test)]`
modules in `src/compositor/sway.rs` and `src/persistence.rs` exercise the
persistence path directly, and is also why `cargo test` runs the suite twice
(once for the `lib` target, once for the `bin` target — both share the same
source, see section 4).

### 2.2 Session identification (`src/identifier.rs`)

`get_current_session()` inspects environment variables
(`XDG_SESSION_TYPE`, `HYPRLAND_INSTANCE_SIGNATURE`, `SWAYSOCK`,
`XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP`, `DESKTOP_SESSION`,
`WAYLAND_DISPLAY`, `DISPLAY`) in a fixed priority order and returns a
`Desktop` enum (`Sway`, `Hyprland`, `Gnome`, `Kde`, `Plasma`, `Xfce`,
`Cosmic`, `Wayland`, `X11`, `Tty`, `Unknown(String)`). Empty/whitespace-only
env values are treated as absent. `XDG_CURRENT_DESKTOP` values containing
both `cosmic` and `sway` (Regolith's actual token,
`Regolith-Wayland:COSMIC:sway`) resolve to `Desktop::Sway` — this is the
literal code path the Regolith session takes. The module has a `#todo`
comment inviting further edge-case hardening; it is not a stub.

### 2.3 Event model (`src/event/mod.rs`, `src/event/input.rs`,
`src/event/shortcuts.rs`)

`Event` is a two-variant enum: `Input(InputEvent)` and
`Shortcut(ShortcutEvent)`. `InputEvent` further splits into `Keyboard`,
`Mouse`, and `TouchPad` variants, each carrying an old→new diff (e.g.
`KeyboardEvent::Layout(String)`, `TouchpadEvent::TapConfig(Option<TapConfig>)`).
`event/input.rs` contains the `from(old, new)` diff functions that compare a
previous and current `cosmic_comp_config` struct and emit only the events for
fields that actually changed — several of these functions carry `#todo`
notes about consolidating into a trait method and about some emitted
sub-events being "redundant when all sub-field events are emitted" (i.e.
known, called-out duplication, not a bug).

### 2.4 Watchers (`src/watcher/`)

Two watcher modules, each registering a `cosmic_config::Config::watch`
callback and translating changed keys into `Event`s pushed onto the shared
channel:

- `watcher::input` (`src/watcher/input.rs`) watches the
  `com.system76.CosmicComp` namespace, tracking four keys today:
  `input_touchpad`, `input_default` (mouse), `xkb_config`, and
  `keyboard_config` (numlock). A long comment block in the file lists 14
  more `cosmic-config` keys (workspaces, autotile, focus-follows-cursor,
  accessibility zoom, etc.) that are recognized as relevant but not yet
  wired up — this is the honest current scope of what cosmolith watches,
  not the full COSMIC config surface.
- `watcher::shortcuts` (`src/watcher/shortcuts.rs`) watches the
  `cosmic_settings_config::shortcuts` namespace, diffs the old/new
  `HashMap<Binding, Action>` on every change, and emits `Add`/`Remove`
  `ShortcutEvent`s for whatever bindings differ.

### 2.5 Compositor abstraction (`src/compositor/mod.rs`,
`src/compositor/input.rs`, `src/compositor/shortcut.rs`)

`Compositor` is the central trait: `init`, `name`, `is_running`, `supports`,
`apply_event`, `reload`, `shutdown`, and (new)
`replay_persisted_config` (default no-op, returns `Ok(0)`). `init_compositor`
matches the detected `Desktop` to a concrete backend (`Sway`, `Hyprland`,
`Kde`, `Gnome`; anything else returns `None`, and `main.rs` then only logs
events instead of applying them).

`compositor::input::Input` and `compositor::shortcut::Shortcut` are
per-domain sub-traits with default `apply_*_event` dispatch methods that
call out to per-setting methods (`keyboard_layout`, `touchpad_tap_config`,
`mouse_scroll_method`, etc.) that each backend implements individually.

### 2.6 Sway backend (`src/compositor/sway.rs`, 586 lines — the backend
actually used by the Regolith session)

`Sway` holds a `Mutex<Option<swayipc::Connection>>`. All per-setting `Input`
methods funnel through one private helper, `run_command(&self, cmd: String)`,
which:

1. Calls `persistence::record_input_line(&cmd)` **unconditionally, before**
   touching the live IPC socket — this is the persistence hook added last
   night (see section 3). If persistence write fails, it's logged to stderr
   and command execution proceeds anyway (persistence failure never blocks
   applying the setting).
2. Lazily opens a `swayipc::Connection` on first use, reconnecting once and
   retrying if the existing connection errors.
3. Runs the command over Sway IPC and logs (not propagates) any
   command-level error Sway itself returns.

`Shortcut::add_shortcut`/`remove_shortcut` build `bindsym`/`unbindsym`
commands via `format_binding` (COSMIC modifier/key → Sway keysym string) and
`format_action` (COSMIC `Shortcut` enum → a Sway `exec`/built-in command
string, e.g. `Shortcut::System(SystemAction::Launcher)` →
`"exec /usr/bin/cosmic-launcher"`). These do **not** go through
`record_input_line`/persistence — only `input <target> <setting> <value>`
lines are persisted (see section 3); keybindings are not currently part of
the generated-config.d contract.

`replay_persisted_config` (the `Compositor` trait override) calls
`persistence::replay_into`, feeding each saved line back through
`self.run_command(...)` — meaning a replayed line is itself re-persisted
(a harmless no-op rewrite of the same line) and sent over IPC.

The Sway backend's own `#[cfg(test)]` module has two focused tests
(`keyboard_layout_change_persists_through_generic_input_directive_path`,
`keyboard_variant_change_persists_through_generic_input_directive_path`)
added in the last commit (`d61a617`) specifically to prove that keyboard
layout/variant persistence needed **no backend-specific code**: it already
flows through the generic `run_command` → `record_input_line` path shared by
every other input setting.

### 2.7 Other backends

- `src/compositor/hyprland.rs` (394 lines) — uses `hyprland-rs`
  (`Keyword::set`) to push settings as Hyprland keywords. Several methods
  are commented out with `// TODO:` notes explaining Hyprland has no
  equivalent keyword (touchpad/mouse enable-disable, calibration, rotation,
  scroll_button, map-to-output, etc.).
- `src/compositor/gnome.rs` (114 lines) — uses `gio::Settings` against GNOME
  schemas (`org.gnome.desktop.peripherals.{touchpad,mouse}`). Has one
  currently-unused helper (`set_double`, flagged by the compiler as dead
  code — see section 4).
- `src/compositor/kde.rs` (122 lines) — shells out to `kwriteconfig6` over a
  `zbus::blocking::Connection`.

None of these three implement `replay_persisted_config` (they get the
trait's default `Ok(0)` no-op), so the generated-config.d persistence
mechanism described below is Sway-specific today.

### 2.8 Error handling (`src/error.rs`)

A single `thiserror`-derived `Error` enum (`ConfigInit`, `ConfigRead`,
`WatcherSetup`, `ChannelLock`, `ChannelSend`) with constructor helpers that
box the underlying source error. Used by the watchers and `main.rs`; the
compositor backends mostly use their own `Box<dyn Error + Send + Sync>`
result aliases (`CompositorResult`, `InputResult`) rather than this enum.

## 3. The `generated-config.d` persistence mechanism (new, last night)

File: `src/persistence.rs` (265 lines, added in commits `4134034` "feat:
persist generated Sway input config to generated-config.d" and `d61a617`
"test: verify keyboard layout/variant reuse generic input persistence").

### 3.1 Why it exists

Regolith's own Sway root config already contains:

```
include $HOME/.config/regolith3/sway/cosmic-settings/generated-config.d/*
```

(in `regolith-wm-config/etc/regolith/sway/config`), but nothing in
cosmolith ever wrote to that directory before this change. Every input
setting cosmolith applies is already phrased as a Sway `input <target>
<setting> <value>` command over IPC — and that exact string is *also* valid
Sway config-file syntax. `src/persistence.rs`'s header comment documents
this directly: the same line sent over IPC can be dropped into a config
file unchanged. Without this, an input setting applied by cosmolith while
it's running only lives in the current Sway process's runtime state; if
Sway (or cosmolith) restarts before `cosmic-config` re-emits that setting,
it's lost.

### 3.2 File location and format

- Directory: `generated_config_dir()` resolves to
  `$XDG_CONFIG_HOME/regolith3/sway/cosmic-settings/generated-config.d/` if
  `XDG_CONFIG_HOME` is set (used by tests to sandbox from the real home
  directory), otherwise `$HOME/.config/regolith3/sway/cosmic-settings/generated-config.d/`.
- File: a single file, `input.conf`, inside that directory.
- Format: plain Sway config syntax, one directive per line, e.g.:

  ```
  # Generated by cosmolith. Do not edit by hand; changes will be overwritten.
  input type:touchpad tap enabled
  input type:keyboard xkb_layout us,fr
  ```

- Only lines that parse as `input <target> <setting> <value...>` are
  persisted (`parse_key` requires the first token to be exactly `"input"`).
  Anything else — the code specifically calls out `bindsym` commands — is
  silently not persisted, since keybindings and other non-`input` commands
  aren't part of this contract yet.
- Directives are keyed by `(target, setting)` (a `DirectiveKey` struct), so
  a later change to the same `target`+`setting` **overwrites** the
  previous line for that key rather than accumulating duplicate/stale
  directives. Storage is a `BTreeMap<DirectiveKey, String>` for
  deterministic ordering.
- Writes are write-then-rename (`input.conf.tmp` written, then
  `fs::rename`d over `input.conf`) so a concurrent Sway config reload never
  observes a partially written file.

### 3.3 Write path

`Sway::run_command` (in `src/compositor/sway.rs`) calls
`persistence::record_input_line(&cmd)` **before** it does anything with the
live IPC socket. This means: the write to disk happens even if there is no
live Sway connection at all (verified directly by the two new
`compositor::sway::tests` — they call `sway.keyboard_layout(...)` with no
Sway socket present, ignore the resulting IPC error, and then assert the
directive landed in `generated-config.d/input.conf` anyway).

### 3.4 Replay-on-start behavior

Two independent mechanisms merge a persisted directive back into a running
config, by design:

1. **Sway's own `include` line** — any `generated-config.d/*` file is
   textually included into Sway's config on Sway's own startup or on
   `swaymsg reload`, with no cosmolith involvement at all.
2. **cosmolith's explicit replay at its own startup** — `main.rs` calls
   `compositor.replay_persisted_config()` once, right after
   `init_compositor`. For the Sway backend this is
   `persistence::replay_into(|line| self.run_command(line))`: it loads every
   saved directive (`load_input_lines`) and re-sends each one through
   `run_command` over the live Sway IPC connection. This covers the case
   where cosmolith itself restarts (e.g. after a crash or manual restart)
   while Sway keeps running and won't re-read its config file on its own —
   without this, cosmolith would only recover state once `cosmic-config`
   re-emitted every value, which it does not do proactively.

`replay_into` returns the count of replayed directives, which `main.rs`
prints (`"Replayed {count} persisted setting(s) from generated-config.d"`)
or logs an error for, but never treats as fatal — a replay failure does not
stop cosmolith from starting.

### 3.5 Test coverage

`src/persistence.rs` has its own `#[cfg(test)]` module (four tests) plus a
shared `test_support::TempConfigHome` helper (env-var-locked, so tests that
touch `XDG_CONFIG_HOME` cannot interleave across threads) that is
specifically exposed as `pub(crate)` so `compositor::sway`'s tests can reuse
the same lock rather than racing on the same process-global env var:

- `generated_config_dir_matches_committed_proposal_path` — asserts the path
  matches the proposal-committed
  `regolith3/sway/cosmic-settings/generated-config.d` layout.
- `write_then_restart_merges_persisted_directive_into_runtime_config` — the
  core round-trip: record a directive, assert the file exists, then assert
  `replay_into` plays back exactly that directive.
- `later_change_to_same_setting_overwrites_instead_of_duplicating` —
  confirms the `DirectiveKey` overwrite semantics.
- `unrelated_commands_are_not_persisted` — confirms a `bindsym` command is
  silently dropped rather than persisted.

## 4. Building and testing locally

Toolchain used on the laptop worktree: `cargo 1.92.0`, `rustc 1.92.0`.
`Cargo.toml` declares no feature flags (`[dependencies]` only — no
`[features]` section), so there is exactly one build/test configuration.
Key external deps: `cosmic-config`/`cosmic-comp-config`/
`cosmic-settings-config` (all pinned to specific upstream git revs),
`swayipc 4.0.0`, `hyprland-rs` (git, branch `master`), `zbus 5.13.2`,
`gio 0.21.5`, `xkbcommon 0.7.0`, `thiserror 2.0.18`.

```sh
cd cosmolith
cargo build --locked      # verified: succeeds, 2 warnings (see below)
cargo test --locked       # verified: 13/13 tests pass (run twice — lib + bin targets)
cargo fmt --check         # verified: NOT clean — pre-existing formatting drift
                           # unrelated to this branch's diff (~1000-line diff,
                           # mostly in src/watcher/shortcuts.rs); do not treat
                           # a clean fmt run as proof of anything until that
                           # drift is addressed separately.
```

Actual verified results at commit `d61a617` (2026-08-14, laptop worktree):

- `cargo build --locked`: succeeds. Two compiler warnings: an unused
  `ShortcutEvent` import in `src/compositor/sway.rs`, and a never-used
  `Gnome::set_double` method (dead code) in `src/compositor/gnome.rs`.
- `cargo test --locked`: **13 passed, 0 failed** in the `lib` target and
  again **13 passed, 0 failed** in the `bin` target (0 doc-tests). Tests
  cover `identifier`, `error`, `watcher::input`, `persistence`, and
  `compositor::sway`.
- `cargo fmt --check`: exits non-zero; the diff is large (~1000 lines) and
  is not confined to files touched by the persistence work, so it is
  pre-existing drift, not a regression from last night's commits — but it
  is real and should be closed out (with `cargo fmt`, reviewed) before
  calling the branch release-ready.

No QEMU/runtime proof is claimed by this document for anything beyond what
section 3.5's unit tests directly assert. Whether a live Sway session
actually picks up a replayed `generated-config.d` directive over a real IPC
socket is a QEMU-level claim that belongs in the vault's testing-proof
notes, not here.

## 5. Known limitations / open items

- **Keyboard layout/variant persistence: verified working.** As documented
  in section 2.6 and 3.5, this needed no dedicated code — it flows through
  the same generic `input <target> <setting> <value>` → `run_command` →
  `record_input_line` path as every other input setting, and this is
  directly asserted by the two tests added in commit `d61a617`.
- **Persistence covers only `input <target> <setting> <value>` directives.**
  Keybindings (`bindsym`/`unbindsym`, built via `Sway::add_shortcut`/
  `remove_shortcut`) are explicitly excluded (`unrelated_commands_are_not_persisted`
  proves a `bindsym` line is dropped, not persisted). If keybinding
  persistence is wanted, it needs separate design — the current
  `DirectiveKey`/`input.conf` scheme is input-setting-specific by
  construction (`parse_key` hard-requires the first token to be `"input"`).
- **Persistence is Sway-only.** `Compositor::replay_persisted_config`
  defaults to a no-op; Hyprland, GNOME, and KDE backends don't call
  `persistence::record_input_line` from their own `run_command`-equivalents
  and don't override the replay hook. Extending persistence to those
  backends means adding the same write-before-apply hook to each one.
- **Watched `cosmic-config` keys are a subset of the full schema.** A
  comment block at the top of `src/watcher/input.rs` explicitly lists 14
  more relevant keys (workspaces, pinned_workspaces, input_touchpad_override,
  input_devices, autotile, autotile_behaviour, active_hint,
  focus_follows_cursor, cursor_follows_focus,
  focus_follows_cursor_delay, descale_xwayland, xwayland_eavesdropping,
  edge_snap_threshold, accessibility_zoom) that are not yet watched. Only
  `input_touchpad`, `input_default`, `xkb_config`, and `keyboard_config` are
  wired up today.
- **Several `Input` trait methods are stubbed out per backend**, each with
  an inline `// TODO:` explaining why (see section 2.6/2.7): e.g. Sway has
  no per-device enable/disable, no touchpad/mouse calibration, no rotation
  angle, no tap-button-map, and no map-to-output support via `input
  type:...`; Hyprland is missing similar per-device and calibration
  primitives; these are upstream compositor limitations noted at the call
  site, not missing cosmolith code.
- **`cargo fmt --check` is not clean** on this branch (pre-existing drift,
  see section 4) — should be resolved and re-verified before any
  release/merge claim.
- **Two compiler warnings** (unused import in `sway.rs`, dead code in
  `gnome.rs`) are present but harmless; worth a follow-up cleanup pass.
- **Error handling is inconsistent between modules.** `src/error.rs`'s
  structured `Error` enum is used by watchers/`main.rs`, but compositor
  backends mostly just print to stderr and return `Ok(())` on
  backend-specific failures (e.g. `Sway::run_command` logs Sway IPC command
  errors rather than surfacing them through `Error`). This is a design
  inconsistency worth resolving, not evaluated further here.
- **No integration/QEMU-level test exists in this repo** for the
  replay-on-restart behavior against a real Sway socket; only the unit-level
  round-trip (write → read back) is proven in-tree. Live-session replay
  proof, if needed, belongs in the vault's `05_Testing_Proof/` QEMU notes,
  not in this crate's test suite.
