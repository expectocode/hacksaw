mod lib;

use lib::parse_args::Opt;
use lib::{
    find_escape_keycode, get_window_at_point, get_window_geom, grab_key, grab_pointer_set_cursor,
    set_shape, set_title, ungrab_key, HacksawResult, CURSOR_GRAB_TRIES,
};
use structopt::StructOpt;
use x11rb::connection::Connection;
use x11rb::protocol::{xproto, Event};

fn min_max(a: i16, b: i16) -> (i16, i16) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn build_guides(
    screen: xproto::Rectangle,
    pt: xproto::Point,
    width: u16,
) -> [xproto::Rectangle; 2] {
    [
        xproto::Rectangle {
            x: pt.x - width as i16 / 2,
            y: screen.x,
            width: width,
            height: screen.height,
        },
        xproto::Rectangle {
            x: screen.y,
            y: pt.y - width as i16 / 2,
            width: screen.width,
            height: width,
        },
    ]
}

fn main() -> Result<(), String> {
    let opt = Opt::from_args();

    let line_width = opt.select_thickness;
    let guide_width = opt.guide_thickness;
    let line_colour = opt.line_colour;
    let format = opt.format;

    let (conn, screen_num) = x11rb::rust_connection::RustConnection::connect(None).unwrap();
    let setup = conn.setup();
    let screen = &setup.roots[screen_num];
    let root = screen.root;

    let window = conn.generate_id().unwrap();

    // TODO fix pointer-grab? bug where hacksaw hangs if mouse held down before run
    if !grab_pointer_set_cursor(&conn, root) {
        return Err(format!(
            "Failed to grab cursor after {} tries, giving up",
            CURSOR_GRAB_TRIES
        ));
    }

    let escape_keycode = find_escape_keycode(&conn);
    grab_key(&conn, root, escape_keycode);

    let screen_rect = xproto::Rectangle {
        x: 0,
        y: 0,
        width: screen.width_in_pixels,
        height: screen.height_in_pixels,
    };

    // TODO event handling for expose/keypress
    let value_list = xproto::CreateWindowAux::new()
        .background_pixel(line_colour)
        .event_mask(
            xproto::EventMask::Exposure
                | xproto::EventMask::KeyPress
                | xproto::EventMask::StructureNotify
                | xproto::EventMask::SubstructureNotify,
        )
        .override_redirect(1);

    xproto::create_window(
        &conn,
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        root,
        screen_rect.x,
        screen_rect.y,
        screen_rect.width,
        screen_rect.height,
        0,
        xproto::WindowClass::InputOutput,
        screen.root_visual,
        &value_list,
    )
    .unwrap()
    .check()
    .unwrap();

    set_title(&conn, window, "hacksaw");

    set_shape(
        &conn,
        window,
        &[xproto::Rectangle {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }],
    );

    xproto::map_window(&conn, window).unwrap().check().unwrap();

    if !opt.no_guides {
        let pointer = xproto::query_pointer(&conn, root).unwrap().reply().unwrap();
        set_shape(
            &conn,
            window,
            &build_guides(
                screen_rect,
                xproto::Point {
                    x: pointer.root_x,
                    y: pointer.root_y,
                },
                guide_width,
            ),
        );
    }

    conn.flush().unwrap();

    let mut start_pt = xproto::Point { x: 0, y: 0 };
    let mut selection = xproto::Rectangle {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    let mut in_selection = false;
    let mut ignore_next_release = false;

    // TODO draw rectangle around window under cursor
    loop {
        let ev = conn
            .wait_for_event()
            .map_err(|_| "Error getting X event, quitting.".to_string())?;

        match ev {
            Event::ButtonPress(button_press) => {
                let detail = button_press.detail;
                if detail == 3 {
                    return Err("Exiting due to right click".into());
                } else {
                    set_shape(&conn, window, &[]);
                    conn.flush().unwrap();
                    start_pt = xproto::Point {
                        x: button_press.event_x,
                        y: button_press.event_y,
                    };

                    in_selection = !(detail == 4 || detail == 5);
                    ignore_next_release = detail == 4 || detail == 5;
                }
            }
            Event::KeyPress(_) => {
                // This will only happen with an escape key since we only grabbed escape
                return Err("Exiting due to ESC key press".into());
            }
            Event::MotionNotify(motion) => {
                let (left_x, right_x) = min_max(motion.event_x, start_pt.x);
                let (top_y, bottom_y) = min_max(motion.event_y, start_pt.y);
                let width = (right_x - left_x) as u16;
                let height = (bottom_y - top_y) as u16;

                // only save the width and height if we are selecting a
                // rectangle, since we then use these (non-zero width/height)
                // to determine if a selection was made.
                selection = if in_selection {
                    xproto::Rectangle {
                        x: left_x,
                        y: top_y,
                        width,
                        height,
                    }
                } else {
                    xproto::Rectangle {
                        x: left_x,
                        y: top_y,
                        width: 0,
                        height: 0,
                    }
                };

                if in_selection {
                    let rects = [
                        // Selection rectangle
                        xproto::Rectangle {
                            x: left_x - line_width as i16,
                            y: top_y,
                            width: line_width,
                            height: height + line_width,
                        },
                        xproto::Rectangle {
                            x: left_x - line_width as i16,
                            y: top_y - line_width as i16,
                            width: width + line_width,
                            height: line_width,
                        },
                        xproto::Rectangle {
                            x: right_x,
                            y: top_y - line_width as i16,
                            width: line_width,
                            height: height + line_width,
                        },
                        xproto::Rectangle {
                            x: left_x,
                            y: bottom_y,
                            width: width + line_width,
                            height: line_width,
                        },
                    ];

                    set_shape(&conn, window, &rects);
                } else if !opt.no_guides {
                    let rects = build_guides(
                        screen_rect,
                        xproto::Point {
                            x: motion.event_x,
                            y: motion.event_y,
                        },
                        guide_width,
                    );

                    set_shape(&conn, window, &rects);
                }

                conn.flush().unwrap();
            }
            Event::ButtonRelease(button_release) => {
                let detail = button_release.detail;
                if detail == 4 || detail == 5 {
                    continue; // Scroll wheel up/down release
                } else if ignore_next_release {
                    ignore_next_release = false;
                    continue;
                } else {
                    break;
                }
                // Move on after mouse released
            }
            _ => continue,
        };
    }

    xproto::ungrab_pointer(&conn, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    ungrab_key(&conn, root, escape_keycode);
    xproto::unmap_window(&conn, window)
        .unwrap()
        .check()
        .unwrap();
    xproto::destroy_window(&conn, window)
        .unwrap()
        .check()
        .unwrap();
    conn.flush().unwrap();

    loop {
        let ev = conn
            .wait_for_event()
            .map_err(|_| "Error getting X event, quitting.".to_string())?;

        match ev {
            x11rb::protocol::Event::UnmapNotify(_) | x11rb::protocol::Event::DestroyNotify(_) => {
                break;
            }
            _ => (),
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(40));

    let result;
    if selection.width == 0 && selection.height == 0 {
        // Grab window under cursor
        result = match get_window_at_point(&conn, root, start_pt, opt.remove_decorations) {
            Some(r) => r,
            None => get_window_geom(&conn, screen.root),
        }
    } else {
        result = HacksawResult {
            window: root,
            rect: selection,
        };
    }

    // Now we have taken coordinates, we print them out
    println!("{}", result.fill_format_string(&format));

    Ok(())
}
