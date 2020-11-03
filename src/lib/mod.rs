pub mod parse_args;
pub mod parse_format;

use self::parse_format::FormatToken;
use x11rb::connection::Connection;
use x11rb::protocol::shape;
use x11rb::protocol::xproto;

pub const CURSOR_GRAB_TRIES: i32 = 5;
const ESC_KEYSYM: u32 = 0xff1b;

/// Since MOD_MASK_ANY is apparently bug-ridden, we instead exploit the fact
/// that the modifier masks NONE to MOD_MASK_5 are 0, 1, 2, 4, 8, ... 128.
/// Then we grab on every possible combination of these masks by iterating
/// through all the integers 0 to 255. This allows us to grab Esc, Shift+Esc,
/// CapsLock+Shift+Esc, or any other combination.
const KEY_GRAB_MASK_MAX: u16 = (xproto::ModMask::M5 as u16 * 2) - 1;

#[derive(Clone, Copy)]
pub struct HacksawResult {
    pub window: u32,
    pub rect: xproto::Rectangle,
}

impl HacksawResult {
    pub fn x(&self) -> i16 {
        self.rect.x
    }
    pub fn y(&self) -> i16 {
        self.rect.y
    }
    pub fn width(&self) -> u16 {
        self.rect.width
    }
    pub fn height(&self) -> u16 {
        self.rect.height
    }

    pub fn relative_to(&self, parent: HacksawResult) -> HacksawResult {
        HacksawResult {
            window: self.window,
            rect: xproto::Rectangle {
                x: parent.x() + self.x(),
                y: parent.y() + self.y(),
                width: self.width(),
                height: self.height(),
            },
        }
    }

    fn contains(&self, point: xproto::Point) -> bool {
        // TODO negative x/y offsets from bottom or right?
        self.x() < point.x
            && self.y() < point.y
            && point.x - self.x() <= self.width() as i16
            && point.y - self.y() <= self.height() as i16
    }

    pub fn fill_format_string(&self, format: &[FormatToken]) -> String {
        format
            .iter()
            .map(|token| match token {
                FormatToken::WindowId => self.window.to_string(),
                FormatToken::Geometry => format!(
                    "{}x{}+{}+{}",
                    self.width(),
                    self.height(),
                    self.x(),
                    self.y(),
                ),
                FormatToken::Width => self.width().to_string(),
                FormatToken::Height => self.height().to_string(),
                FormatToken::X => self.x().to_string(),
                FormatToken::Y => self.y().to_string(),
                FormatToken::Literal(s) => s.to_string(),
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

pub fn set_shape<C: Connection>(conn: &C, window: xproto::Window, rects: &[xproto::Rectangle]) {
    shape::rectangles(
        conn,
        shape::SO::Set,
        shape::SK::Bounding,
        xproto::ClipOrdering::Unsorted,
        window,
        0,
        0,
        &rects,
    )
    .unwrap()
    .check()
    .unwrap();
}

pub fn set_title<C: Connection>(conn: &C, window: xproto::Window, title: &str) {
    xproto::change_property(
        conn,
        xproto::PropMode::Replace,
        window,
        xproto::AtomEnum::WM_NAME,
        xproto::AtomEnum::STRING,
        8,
        title.len() as u32,
        title.as_bytes(),
    )
    .unwrap()
    .check()
    .unwrap();
}

pub fn grab_pointer_set_cursor<C: Connection>(conn: &C, root: u32) -> bool {
    let font = conn.generate_id().unwrap();
    xproto::open_font(conn, font, b"cursor")
        .unwrap()
        .check()
        .unwrap();

    // TODO: create cursor with a Pixmap
    // https://stackoverflow.com/questions/40578969/how-to-create-a-cursor-in-x11-from-raw-data-c
    let cursor = conn.generate_id().unwrap();
    xproto::create_glyph_cursor(conn, cursor, font, font, 0, 30, 0, 0, 0, 0, 0, 0)
        .unwrap()
        .check()
        .unwrap();

    for i in 0..CURSOR_GRAB_TRIES {
        let reply = xproto::grab_pointer(
            conn,
            true,
            root,
            (xproto::EventMask::ButtonRelease
                | xproto::EventMask::ButtonPress
                | xproto::EventMask::ButtonMotion
                | xproto::EventMask::PointerMotion) as u16,
            xproto::GrabMode::Async,
            xproto::GrabMode::Async,
            x11rb::NONE,
            cursor,
            x11rb::CURRENT_TIME,
        )
        .unwrap()
        .reply()
        .unwrap();

        if reply.status == xproto::GrabStatus::Success {
            return true;
        } else if i < CURSOR_GRAB_TRIES - 1 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    false
}

pub fn find_escape_keycode<C: Connection>(conn: &C) -> xproto::Keycode {
    // https://stackoverflow.com/questions/18689863/obtain-keyboard-layout-and-keysyms-with-xcb
    let setup = conn.setup();
    let cookie = xproto::get_keyboard_mapping(
        conn,
        setup.min_keycode,
        setup.max_keycode - setup.min_keycode + 1,
    )
    .unwrap();
    let reply = cookie.reply().expect("failed to get keyboard mapping");

    let escape_index = reply
        .keysyms
        .iter()
        .position(|&keysym| keysym == ESC_KEYSYM)
        .expect("failed to find escape keysym");
    (escape_index / reply.keysyms_per_keycode as usize) as u8 + setup.min_keycode
}

pub fn grab_key<C: Connection>(conn: &C, root: u32, keycode: u8) {
    for mask in 0..=KEY_GRAB_MASK_MAX {
        xproto::grab_key(
            conn,
            true,
            root,
            mask,
            keycode,
            xproto::GrabMode::Async,
            xproto::GrabMode::Async,
        )
        .unwrap()
        .check()
        .unwrap();
    }
}

pub fn ungrab_key<C: Connection>(conn: &C, root: u32, keycode: u8) {
    for mask in 0..=KEY_GRAB_MASK_MAX {
        xproto::ungrab_key(conn, keycode, root, mask)
            .unwrap()
            .check()
            .unwrap();
    }
}

fn viewable<C: Connection>(conn: &C, win: xproto::Window) -> bool {
    let attrs = xproto::get_window_attributes(conn, win)
        .unwrap()
        .reply()
        .unwrap();
    attrs.map_state == xproto::MapState::Viewable
}

pub fn input_output<C: Connection>(conn: &C, win: xproto::Window) -> bool {
    let attrs = xproto::get_window_attributes(conn, win)
        .unwrap()
        .reply()
        .unwrap();
    attrs.class == xproto::WindowClass::InputOutput
}

pub fn get_window_geom<C: Connection>(conn: &C, win: xproto::Window) -> HacksawResult {
    let geom = xproto::get_geometry(conn, win).unwrap().reply().unwrap();

    HacksawResult {
        window: win,
        rect: xproto::Rectangle {
            x: geom.x,
            y: geom.y,
            width: geom.width + 2 * geom.border_width,
            height: geom.height + 2 * geom.border_width,
        },
    }
}

pub fn get_window_at_point<C: Connection>(
    conn: &C,
    win: xproto::Window,
    pt: xproto::Point,
    remove_decorations: u32,
) -> Option<HacksawResult> {
    let tree = xproto::query_tree(conn, win).unwrap().reply().unwrap();
    let children = tree
        .children
        .iter()
        .filter(|&child| viewable(conn, *child))
        .filter(|&child| input_output(conn, *child))
        .filter_map(|&child| {
            let geom = get_window_geom(conn, child);
            if geom.contains(pt) {
                Some(geom)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if children.is_empty() {
        return None;
    }

    let mut window = children[children.len() - 1];
    for _ in 0..remove_decorations {
        let tree = xproto::query_tree(conn, window.window)
            .unwrap()
            .reply()
            .unwrap();
        if tree.children.is_empty() {
            break;
        }
        let firstborn = tree.children[0];
        window = get_window_geom(conn, firstborn).relative_to(window);
    }

    Some(window)
}
