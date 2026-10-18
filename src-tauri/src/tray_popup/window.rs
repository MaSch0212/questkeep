//! The popup's Win32 window: one per opening, on its own thread with its own message loop, so the
//! Tauri event loop is never blocked and nothing stays alive after it closes. It closes on Escape,
//! when it loses activation (a click elsewhere), or after an item was chosen; the chosen action is
//! run once the window is gone.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    COLORREF, D2DERR_RECREATE_TARGET, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_U, D2D1_ALPHA_MODE_UNKNOWN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    ID2D1HwndRenderTarget,
};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    ScreenToClient, ValidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME,
    VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    IDC_ARROW, LoadCursorW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW,
    SetForegroundWindow, ShowWindow, TranslateMessage, WA_INACTIVE, WM_ACTIVATE, WM_CLOSE,
    WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{Result, w};

use super::layout::{self, Action, Layout, Nav, ScreenRect, navigate};
use super::render::{Bitmaps, Frame, Renderer, ViewState};
use super::{PopupData, Prepared, prepare};

/// `WM_MOUSELEAVE` (declared in a `windows` feature this crate does not otherwise need).
const WM_MOUSELEAVE: u32 = 0x02A3;
const CLASS_NAME: windows::core::PCWSTR = w!("QuestkeepTrayPopup");
/// Distance from the taskbar and the screen edges, in logical pixels.
const SCREEN_GAP: f32 = 12.0;
/// A tray click right after the popup closed is the click that closed it (it deactivated the
/// popup on mouse down); it must not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// The open popup's window handle, if any.
static OPEN: Mutex<Option<isize>> = Mutex::new(None);
static LAST_CLOSED: Mutex<Option<Instant>> = Mutex::new(None);
static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct State {
    renderer: Renderer,
    target: Option<ID2D1HwndRenderTarget>,
    bitmaps: Bitmaps,
    data: PopupData,
    layout: Layout,
    images: Prepared,
    scale: f32,
    scroll: f32,
    view: ViewState,
    tracking_leave: bool,
    action: Option<Action>,
}

impl State {
    fn relayout(&mut self) {
        self.layout = layout::layout(&self.data.content(), self.scroll, |t| {
            self.renderer.measure(t)
        });
        self.scroll = self.scroll.clamp(0.0, self.layout.max_scroll);
    }
}

/// Opens the popup next to the tray icon at `anchor` (physical pixels), or closes it when it is
/// already open. `on_action` runs on the popup thread after the popup closed.
pub fn show(anchor: ScreenRect, data: PopupData, on_action: impl Fn(Action) + Send + 'static) {
    let open = *OPEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(hwnd) = open {
        // SAFETY: posting to a window handle is safe even if it is already gone.
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        return;
    }
    let recently = LAST_CLOSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if recently {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("tray-popup".into())
        .spawn(move || {
            let action = match run(anchor, data) {
                Ok(action) => action,
                Err(err) => {
                    tracing::error!(%err, "tray popup failed");
                    None
                }
            };
            *OPEN
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            *LAST_CLOSED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
            if let Some(action) = action {
                on_action(action);
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not open the tray popup");
    }
}

fn register_class() -> Result<()> {
    if CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    // SAFETY: registering a window class with a static name and a valid window procedure.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            CLASS_REGISTERED.store(false, Ordering::SeqCst);
            return Err(windows::core::Error::from_thread());
        }
    }
    Ok(())
}

/// Monitor work area and scale factor for the tray icon's monitor.
fn monitor_for(anchor: ScreenRect) -> (ScreenRect, f32) {
    let point = POINT {
        x: anchor.x + anchor.w / 2,
        y: anchor.y + anchor.h / 2,
    };
    // SAFETY: querying monitor information into owned structs.
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let work = if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let r = info.rcWork;
            ScreenRect {
                x: r.left,
                y: r.top,
                w: r.right - r.left,
                h: r.bottom - r.top,
            }
        } else {
            ScreenRect {
                x: 0,
                y: 0,
                w: 1280,
                h: 720,
            }
        };
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96;
        }
        (work, dpi_x as f32 / 96.0)
    }
}

fn run(anchor: ScreenRect, data: PopupData) -> Result<Option<Action>> {
    register_class()?;
    let (work, scale) = monitor_for(anchor);
    let renderer = Renderer::new()?;
    let layout = layout::layout(&data.content(), 0.0, |t| renderer.measure(t));
    let images = prepare(&data, &layout, scale);
    let (w, h) = (
        (layout.width * scale).round() as i32,
        (layout.height * scale).round() as i32,
    );
    let (x, y) = layout::place(anchor, work, w, h, (SCREEN_GAP * scale).round() as i32);

    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            renderer,
            target: None,
            bitmaps: Bitmaps::default(),
            data,
            layout,
            images,
            scale,
            scroll: 0.0,
            view: ViewState::default(),
            tracking_leave: false,
            action: None,
        });
    });

    // SAFETY: creating and showing a window owned by this thread; all pointers are valid for the
    // calls; the message loop below runs until the window is destroyed.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS_NAME,
            w!("Questkeep"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        style_window(hwnd);
        *OPEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(hwnd.0 as isize);
        let _ = ShowWindow(hwnd, SW_SHOW);
        // The shell lets the tray icon's process take the foreground after a click on its icon;
        // without activation the popup could not notice clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(STATE.with(|s| s.borrow_mut().take().and_then(|state| state.action)))
}

/// Windows 11 look: rounded corners, dark frame, a border in the theme's border color. Older
/// systems ignore the attributes.
fn style_window(hwnd: HWND) {
    let corner = DWMWCP_ROUND;
    let dark: i32 = 1;
    // COLORREF is 0x00BBGGRR; the theme border is #2a2f3a.
    let border = COLORREF(0x003a_2f2a);
    // SAFETY: each attribute gets a pointer to a value of the documented type and size.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corner as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of_val(&corner) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const i32).cast(),
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&border as *const COLORREF).cast(),
            4,
        );
    }
}

/// What a message handler wants done after the state borrow is released (destroying the window
/// re-enters the window procedure).
enum Effect {
    None,
    Redraw,
    Close,
}

fn with_state(f: impl FnOnce(&mut State) -> Effect) -> Effect {
    STATE.with(|s| match s.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map_or(Effect::None, f),
        Err(_) => Effect::None,
    })
}

fn client_point(lparam: LPARAM, scale: f32) -> (f32, f32) {
    let x = (lparam.0 & 0xffff) as u16 as i16;
    let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16;
    (f32::from(x) / scale, f32::from(y) / scale)
}

fn update_hover(state: &mut State, x: f32, y: f32) -> Effect {
    let hover = state.layout.hit(x, y);
    if hover == state.view.hover {
        return Effect::None;
    }
    state.view.hover = hover;
    Effect::Redraw
}

/// Hover under the current cursor position (after scrolling moved the items).
fn hover_at_cursor(hwnd: HWND, state: &mut State) -> Effect {
    let mut p = POINT::default();
    // SAFETY: reading the cursor position into an owned struct.
    unsafe {
        if GetCursorPos(&mut p).is_err() || !ScreenToClient(hwnd, &mut p).as_bool() {
            return Effect::None;
        }
    }
    update_hover(state, p.x as f32 / state.scale, p.y as f32 / state.scale)
}

fn paint(hwnd: HWND, state: &mut State) -> Result<()> {
    if state.target.is_none() {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_UNKNOWN,
                alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
            },
            dpiX: 96.0 * state.scale,
            dpiY: 96.0 * state.scale,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let mut rect = RECT::default();
        // SAFETY: valid window handle and owned output struct.
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect)? };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: (rect.right - rect.left).max(1) as u32,
                height: (rect.bottom - rect.top).max(1) as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // SAFETY: creating a render target for this thread's window.
        let target = unsafe {
            state
                .renderer
                .factory()
                .CreateHwndRenderTarget(&props, &hwnd_props)?
        };
        state.target = Some(target);
        state.bitmaps.clear();
    }
    let Some(target) = state.target.clone() else {
        return Ok(());
    };
    // SAFETY: drawing between BeginDraw and EndDraw on this thread's target.
    let result = unsafe {
        target.BeginDraw();
        let frame = Frame {
            layout: &state.layout,
            data: &state.data,
            images: &state.images,
            state: &state.view,
            scale: state.scale,
        };
        let drawn = state.renderer.draw(&target, &frame, &mut state.bitmaps);
        let ended = target.EndDraw(None, None);
        drawn.and(ended)
    };
    if let Err(err) = result {
        if err.code() == D2DERR_RECREATE_TARGET {
            // The GPU device was lost: draw again with a new target.
            state.target = None;
            state.bitmaps.clear();
            // SAFETY: requesting a repaint of our own window.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        } else {
            return Err(err);
        }
    }
    Ok(())
}

fn activate(state: &mut State, index: usize) -> Effect {
    match state.layout.items.get(index) {
        Some(item) => {
            state.action = Some(item.action.clone());
            Effect::Close
        }
        None => Effect::None,
    }
}

fn on_key(state: &mut State, key: u16) -> Effect {
    let nav = match key {
        k if k == VK_ESCAPE.0 => return Effect::Close,
        k if k == VK_RETURN.0 || k == VK_SPACE.0 => {
            return match state.view.focus {
                Some(i) => activate(state, i),
                None => Effect::None,
            };
        }
        k if k == VK_UP.0 => Nav::Up,
        k if k == VK_DOWN.0 => Nav::Down,
        k if k == VK_LEFT.0 => Nav::Left,
        k if k == VK_RIGHT.0 => Nav::Right,
        k if k == VK_HOME.0 => Nav::First,
        k if k == VK_END.0 => Nav::Last,
        k if k == VK_TAB.0 => {
            // SAFETY: reading the keyboard state of this thread.
            let shift = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
            if shift { Nav::Previous } else { Nav::Next }
        }
        _ => return Effect::None,
    };
    // The first key press starts at the hovered item, if any.
    let from = if state.view.show_focus {
        state.view.focus
    } else {
        state.view.hover
    };
    let focus = if !state.view.show_focus && from.is_none() {
        navigate(&state.layout, None, nav)
    } else {
        navigate(&state.layout, from, nav)
    };
    state.view.show_focus = true;
    state.view.focus = focus;
    if let Some(i) = focus {
        state.scroll = state.layout.reveal(i, state.scroll);
        state.relayout();
    }
    Effect::Redraw
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let effect = match msg {
        WM_PAINT => {
            let _ = with_state(|state| {
                if let Err(err) = paint(hwnd, state) {
                    tracing::warn!(%err, "could not draw the tray popup");
                }
                Effect::None
            });
            // SAFETY: marks the window as painted.
            unsafe {
                let _ = ValidateRect(Some(hwnd), None);
            }
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_MOUSEMOVE => with_state(|state| {
            if !state.tracking_leave {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: asks for WM_MOUSELEAVE on our own window.
                state.tracking_leave = unsafe { TrackMouseEvent(&mut track) }.is_ok();
            }
            let (x, y) = client_point(lparam, state.scale);
            update_hover(state, x, y)
        }),
        WM_MOUSELEAVE => with_state(|state| {
            state.tracking_leave = false;
            state.view.hover = None;
            state.view.pressed = None;
            Effect::Redraw
        }),
        WM_LBUTTONDOWN => with_state(|state| {
            let (x, y) = client_point(lparam, state.scale);
            state.view.pressed = state.layout.hit(x, y);
            Effect::Redraw
        }),
        WM_LBUTTONUP => with_state(|state| {
            let (x, y) = client_point(lparam, state.scale);
            let hit = state.layout.hit(x, y);
            let pressed = state.view.pressed.take();
            match hit {
                Some(i) if pressed == Some(i) => activate(state, i),
                _ => Effect::Redraw,
            }
        }),
        WM_MOUSEWHEEL => with_state(|state| {
            let delta = f32::from(((wparam.0 >> 16) & 0xffff) as u16 as i16);
            let before = state.scroll;
            state.scroll = (state.scroll - delta / 120.0 * layout::ROW_HEIGHT)
                .clamp(0.0, state.layout.max_scroll);
            if (state.scroll - before).abs() < f32::EPSILON {
                return Effect::None;
            }
            state.relayout();
            let _ = hover_at_cursor(hwnd, state);
            Effect::Redraw
        }),
        WM_KEYDOWN => with_state(|state| on_key(state, wparam.0 as u16)),
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 == WA_INACTIVE => Effect::Close,
        WM_CLOSE | WM_DISPLAYCHANGE | WM_DPICHANGED => Effect::Close,
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        // SAFETY: default handling for everything else.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    };
    match effect {
        Effect::None => {}
        // SAFETY: requesting a repaint of our own window.
        Effect::Redraw => unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        },
        // SAFETY: destroying our own window on its thread (no state borrow is held here).
        Effect::Close => unsafe {
            let _ = DestroyWindow(hwnd);
        },
    }
    LRESULT(0)
}
