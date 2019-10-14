pub mod parse_args;
pub mod parse_format;

use self::parse_format::FormatToken;
use xcb::shape;

pub const CURSOR_GRAB_TRIES: i32 = 5;

#[derive(Clone, Copy)]
pub struct HacksawResult {
    pub window: u32,
    pub rect: xcb::Rectangle,
}

impl HacksawResult {
    pub fn x(&self) -> i16 {
        return self.rect.x();
    }
    pub fn y(&self) -> i16 {
        return self.rect.y();
    }
    pub fn width(&self) -> u16 {
        return self.rect.width();
    }
    pub fn height(&self) -> u16 {
        return self.rect.height();
    }

    pub fn relative_to(&self, parent: HacksawResult) -> HacksawResult {
        HacksawResult {
            window: self.window,
            rect: xcb::Rectangle::new(
                parent.x() + self.x(),
                parent.y() + self.y(),
                self.width(),
                self.height(),
            ),
        }
    }

    fn contains(&self, point: xcb::Point) -> bool {
        // TODO negative x/y offsets from bottom or right?
        self.x() < point.x()
            && self.y() < point.y()
            && point.x() - self.x() <= self.width() as i16
            && point.y() - self.y() <= self.height() as i16
    }
}

pub fn fill_format_string(format: Vec<FormatToken>, result: HacksawResult) -> String {
    format
        .into_iter()
        .map(|token| match token {
            FormatToken::WindowId => result.window.to_string(),
            FormatToken::Geometry => format!(
                "{}x{}+{}+{}",
                result.width(),
                result.height(),
                result.x(),
                result.y(),
            ),
            FormatToken::Width => result.width().to_string(),
            FormatToken::Height => result.height().to_string(),
            FormatToken::X => result.x().to_string(),
            FormatToken::Y => result.y().to_string(),
            FormatToken::Literal(s) => s,
        })
        .collect::<Vec<_>>()
        .join("")
}

pub fn set_shape(conn: &xcb::Connection, window: xcb::Window, rects: &[xcb::Rectangle]) {
    shape::rectangles(
        &conn,
        shape::SO_SET as u8,
        shape::SK_BOUNDING as u8,
        0,
        window,
        0,
        0,
        &rects,
    );
}

pub fn set_title(conn: &xcb::Connection, window: xcb::Window, title: &str) {
    xcb::change_property(
        &conn,
        xcb::PROP_MODE_REPLACE as u8,
        window,
        xcb::ATOM_WM_NAME,
        xcb::ATOM_STRING,
        8,
        title.as_bytes(),
    );
}

pub fn grab_pointer_set_cursor(conn: &xcb::Connection, root: u32) -> bool {
    let font = conn.generate_id();
    xcb::open_font(&conn, font, "cursor");

    // TODO: create cursor with a Pixmap
    // https://stackoverflow.com/questions/40578969/how-to-create-a-cursor-in-x11-from-raw-data-c
    let cursor = conn.generate_id();
    xcb::create_glyph_cursor(&conn, cursor, font, font, 0, 30, 0, 0, 0, 0, 0, 0);

    for i in 0..CURSOR_GRAB_TRIES {
        let reply = xcb::grab_pointer(
            &conn,
            true,
            root,
            (xcb::EVENT_MASK_BUTTON_RELEASE
                | xcb::EVENT_MASK_BUTTON_PRESS
                | xcb::EVENT_MASK_BUTTON_MOTION
                | xcb::EVENT_MASK_POINTER_MOTION) as u16,
            xcb::GRAB_MODE_ASYNC as u8,
            xcb::GRAB_MODE_ASYNC as u8,
            xcb::NONE,
            cursor,
            xcb::CURRENT_TIME,
        )
        .get_reply()
        .unwrap();

        if reply.status() as u32 == xcb::GRAB_STATUS_SUCCESS {
            return true;
        } else if i < CURSOR_GRAB_TRIES - 1 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    false
}

fn viewable(conn: &xcb::Connection, win: xcb::Window) -> bool {
    let attrs = xcb::get_window_attributes(conn, win).get_reply().unwrap();
    (attrs.map_state() & xcb::MAP_STATE_VIEWABLE as u8) != 0
}

pub fn input_output(conn: &xcb::Connection, win: xcb::Window) -> bool {
    let attrs = xcb::get_window_attributes(conn, win).get_reply().unwrap();
    (attrs.class() & xcb::WINDOW_CLASS_INPUT_OUTPUT as u16) != 0
}

pub fn get_window_geom(conn: &xcb::Connection, win: xcb::Window) -> HacksawResult {
    let geom = xcb::get_geometry(conn, win).get_reply().unwrap();

    HacksawResult {
        window: win,
        rect: xcb::Rectangle::new(
            geom.x(),
            geom.y(),
            geom.width() + 2 * geom.border_width(),
            geom.height() + 2 * geom.border_width(),
        ),
    }
}

pub fn get_window_at_point(
    conn: &xcb::Connection,
    win: xcb::Window,
    pt: xcb::Point,
    remove_decorations: u32,
) -> Option<HacksawResult> {
    let tree = xcb::query_tree(conn, win).get_reply().unwrap();
    let children = tree
        .children()
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

    if children.len() == 0 {
        return None;
    }

    let mut window = children[children.len() - 1];
    for _ in 0..remove_decorations {
        let tree = xcb::query_tree(conn, window.window).get_reply().unwrap();
        if tree.children_len() == 0 {
            break;
        }
        let firstborn = tree.children()[0];
        window = get_window_geom(conn, firstborn).relative_to(window);
    }

    Some(window)
}
