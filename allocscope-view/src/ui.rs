/*
    allocscope  -  a memory tracking tool
    Copyright (C) 2023  Matt Kimball

    This program is free software: you can redistribute it and/or modify it
    under the terms of the GNU General Public License as published by the
    Free Software Foundation, either version 3 of the License, or (at your
    option) any later version.

    This program is distributed in the hope that it will be useful, but
    WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License
    for more details.

    You should have received a copy of the GNU General Public License along
    with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::report;
use crate::rows;
use crate::trace;
use pancurses;
use std::collections;
use std::error::Error;
use std::time;

// State data relevant to the curses UI.
struct UIState {
    // The connectin to the SQLite database for the trace.
    trace: trace::Trace,

    // The ncurses screen window.
    screen: pancurses::Window,

    // If true, the UI is exiting.
    exited: bool,

    // The currently displayed rows.
    display_rows: Vec<rows::StackEntryRow>,

    // The number of rows skipped before the first row displayed on screen.
    scroll_offset: i64,

    // The number of character columns skipped in the function tree display.
    column_offset: i64,

    // The index of the currently selected row.
    selected_row: i64,

    // The ids of stack entry rows which have been collapsed.
    collapsed: collections::HashSet<trace::StackEntryId>,

    // The current sort mode for the UI.
    sort_mode: rows::SortMode,
}

// Print a column header.
fn print_header(screen: &pancurses::Window, text: &str, selected: bool) {
    if selected {
        screen.attroff(pancurses::COLOR_PAIR(3));
        screen.attron(pancurses::COLOR_PAIR(2));
    }

    screen.printw(text);

    if selected {
        screen.attroff(pancurses::COLOR_PAIR(2));
        screen.attron(pancurses::COLOR_PAIR(3));
    }
}

// Print a keyboard shortcut.
fn print_key(screen: &pancurses::Window, column_limit: usize, key: &str, description: &str) {
    let cur_x = screen.get_cur_x();
    if cur_x as usize + key.len() + description.len() > column_limit {
        return;
    }

    screen.attroff(pancurses::COLOR_PAIR(3));
    screen.printw(key);
    screen.attron(pancurses::COLOR_PAIR(3));
    screen.printw(" ");
    screen.printw(description);

    while screen.get_cur_x() < cur_x + 8 {
        screen.printw(" ");
    }
}

impl UIState {
    // Construct a new curses UI state.
    fn new(trace: trace::Trace, screen: pancurses::Window) -> UIState {
        pancurses::noecho();
        pancurses::curs_set(0);
        pancurses::start_color();
        pancurses::init_pair(1, pancurses::COLOR_WHITE, pancurses::COLOR_BLACK);
        pancurses::init_pair(2, pancurses::COLOR_WHITE, pancurses::COLOR_BLUE);
        pancurses::init_pair(3, pancurses::COLOR_BLACK, pancurses::COLOR_GREEN);
        screen.keypad(true);

        UIState {
            trace,
            screen,
            exited: false,
            display_rows: Vec::new(),
            scroll_offset: 0,
            column_offset: 0,
            selected_row: 0,
            collapsed: collections::HashSet::new(),
            sort_mode: rows::SortMode::Bytes,
        }
    }

    // Generate and cache currently displayed rows, using the current screen
    // size, scroll offset and sort mode.
    fn generate_display_rows(&mut self) -> Result<(), Box<dyn Error>> {
        let max_rows = self.screen.get_max_y() as usize - 1;
        let mut transaction = trace::Transaction::new(&self.trace)?;

        self.display_rows = rows::iter_stackentry_rows(
            &mut transaction,
            self.sort_mode,
            Some(&self.collapsed),
            self.scroll_offset as usize,
            max_rows,
        )?;

        Ok(())
    }

    // Draw the header for the stackentry related columns.
    fn draw_stack_header(&self) {
        self.screen.mv(0, 0);
        self.screen.attron(pancurses::COLOR_PAIR(3));

        print_header(
            &self.screen,
            "BYTES",
            self.sort_mode == rows::SortMode::Bytes,
        );
        self.screen.printw(" ");
        print_header(
            &self.screen,
            "BLOCK",
            self.sort_mode == rows::SortMode::Blocks,
        );
        self.screen.printw(" ");
        print_header(
            &self.screen,
            "LEAKS",
            self.sort_mode == rows::SortMode::Leaks,
        );
        self.screen.printw("   ");
        print_header(&self.screen, "Function", false);

        let width = self.screen.get_max_x();
        let mut support_link = format!(
            "https://support.mkimball.net/  {} ",
            env!("CARGO_PKG_VERSION")
        );
        while (support_link.len() as i32) < width - self.screen.get_cur_x() {
            support_link = " ".to_string() + &support_link;
        }
        self.screen.printw(support_link);

        self.screen.attroff(pancurses::COLOR_PAIR(3));
    }

    // Draw the keyboard help.
    fn draw_key_help(&self) {
        let width = self.screen.get_max_x();
        let height = self.screen.get_max_y();

        self.screen.mv(height - 1, 0);
        self.screen.attron(pancurses::COLOR_PAIR(3));

        print_key(&self.screen, width as usize, "F5", "Sort");

        let cur_x = self.screen.get_cur_x();
        let mut fill = "".to_string();
        while fill.len() < (width - cur_x) as usize {
            fill = fill + " ";
        }
        self.screen.printw(fill);

        self.screen.attroff(pancurses::COLOR_PAIR(3));
    }

    // Draw stack entry rows.
    fn draw_stackentry_rows(&self, rows: &mut dyn Iterator<Item = &rows::StackEntryRow>) {
        let mut row: i64 = 0;

        let width = self.screen.get_max_x() as usize;
        let height = self.screen.get_max_y() as usize;
        while let Some(entry) = rows.next() {
            if row + 2 >= height as i64 {
                break;
            }

            let function_str = report::format_function_tree_row(Some(&self.collapsed), &entry);
            let mut function_substr = "";
            if self.column_offset < function_str.len() as i64 {
                function_substr = &function_str.as_str()[self.column_offset as usize..];
            }

            let mut str = format!(
                "{} {} {} {}",
                report::format_table_value(entry.maximum_size, 1024),
                report::format_table_value(entry.total_blocks, 1000),
                report::format_table_value(entry.unfreed_blocks, 1000),
                function_substr,
            );
            while str.len() < width {
                str = str + " ";
            }

            let mut selected = false;
            if self.selected_row == row + self.scroll_offset {
                selected = true;

                self.screen.attron(pancurses::COLOR_PAIR(2));
                self.screen.attron(pancurses::A_BOLD);
            }

            self.screen.mv(row as i32 + 1, 0);
            self.screen.printw(str);

            if selected {
                self.screen.attroff(pancurses::A_BOLD);
                self.screen.attroff(pancurses::COLOR_PAIR(2));
            }

            row += 1;
        }
    }

    // An unexpected error has occurred while generating the display.
    // Draw it.
    fn draw_error(&mut self, err: Box<dyn Error>) {
        let err_str = format!("{:?}", err);
        self.screen.mv(self.screen.get_max_y() - 1, 0);
        self.screen.printw(err_str);
    }

    // Redraw all components of the user interface.
    fn draw(&mut self, report_perf: bool) {
        let start_draw_time = time::Instant::now();
        self.screen.erase();

        self.draw_stack_header();
        match self.generate_display_rows() {
            Ok(()) => {
                self.draw_stackentry_rows(&mut self.display_rows.iter());
            }
            Err(err) => self.draw_error(err),
        }
        self.draw_key_help();
        let end_draw_time = time::Instant::now();

        if report_perf {
            let time_str = format!("draw: {:.2?} ", end_draw_time - start_draw_time);
            let rows = self.screen.get_max_y();
            self.screen.mv(rows - 1, 0);
            self.screen.printw(time_str);
        }

        self.screen.refresh();
    }

    // Adjust the scroll offset such that the currently selected row is shown.
    fn scroll_to_selection(&mut self) {
        let rows = self.screen.get_max_y() as i64 - 2;

        if self.scroll_offset > self.selected_row {
            self.scroll_offset = self.selected_row;
        }
        if self.scroll_offset + rows <= self.selected_row {
            self.scroll_offset = self.selected_row - rows + 1;
        }
    }

    // Respond to a down keypress.
    fn on_move_down(&mut self) {
        if self.selected_row < self.scroll_offset + self.display_rows.len() as i64 - 1 {
            self.selected_row += 1;
        }
        self.scroll_to_selection();
    }

    // Respond to an up keypress.
    fn on_move_up(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        }
        self.scroll_to_selection();
    }

    // Respond to a left keypress.
    fn on_move_left(&mut self) {
        self.column_offset = std::cmp::max(self.column_offset - 8, 0);
    }

    // Respond to a right keypress.
    fn on_move_right(&mut self) {
        self.column_offset += 8;
    }

    // Respond to a page down keypress.
    fn on_page_down(&mut self) {
        let rows = self.screen.get_max_y() as i64 - 2;

        if self.selected_row == self.scroll_offset + rows - 1
            && self.display_rows.len() as i64 > rows
        {
            self.scroll_offset += rows;
        }
        self.selected_row = self.scroll_offset + rows - 1;

        match self.generate_display_rows() {
            Ok(()) => {
                let display_count = self.display_rows.len() as i64;
                if self.selected_row > self.scroll_offset + display_count - 1 {
                    self.selected_row = self.scroll_offset + display_count - 1;
                }
            }
            _ => {}
        }
    }

    // Respond to a page up keypress.
    fn on_page_up(&mut self) {
        let rows = self.screen.get_max_y() as i64 - 2;

        if self.selected_row == self.scroll_offset {
            self.scroll_offset = std::cmp::max(self.scroll_offset - rows, 0);
        }
        self.selected_row = self.scroll_offset;
    }

    // On a home keypress, scroll to the top.
    fn on_home(&mut self) {
        self.selected_row = 0;
        self.scroll_offset = 0;
    }

    // On an end keypress, scroll to the bottom.
    fn on_end(&mut self) {
        let display_rows = self.screen.get_max_y() as i64 - 2;
        if let Ok(mut transaction) = trace::Transaction::new(&self.trace) {
            if let Ok(total_rows) = rows::count_rows(&mut transaction, Some(&self.collapsed)) {
                self.selected_row = total_rows as i64 - 1;
                self.scroll_offset = std::cmp::max(self.selected_row - display_rows + 1, 0);
            }
        }
    }

    // Collapse or expand the currently selected row.
    fn on_toggle_collapse(&mut self) {
        if let Some(row) = self
            .display_rows
            .get((self.selected_row - self.scroll_offset) as usize)
        {
            if self.collapsed.contains(&row.id) {
                self.collapsed.remove(&row.id);
            } else {
                self.collapsed.insert(row.id);
            }
        }
    }

    // Toggle through the sort modes.
    fn on_next_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            rows::SortMode::None => rows::SortMode::Bytes,
            rows::SortMode::Bytes => rows::SortMode::Blocks,
            rows::SortMode::Blocks => rows::SortMode::Leaks,
            rows::SortMode::Leaks => rows::SortMode::None,
        }
    }

    // Handle the next key pressed.
    fn handle_input(&mut self) {
        if let Some(c) = self.screen.getch() {
            match c {
                pancurses::Input::Character(' ') => self.on_toggle_collapse(),
                pancurses::Input::Character('q') => self.exited = true,
                pancurses::Input::KeyDown => self.on_move_down(),
                pancurses::Input::KeyUp => self.on_move_up(),
                pancurses::Input::KeyLeft => self.on_move_left(),
                pancurses::Input::KeyRight => self.on_move_right(),
                pancurses::Input::KeyNPage => self.on_page_down(),
                pancurses::Input::KeyPPage => self.on_page_up(),
                pancurses::Input::KeyHome => self.on_home(),
                pancurses::Input::KeyEnd => self.on_end(),
                pancurses::Input::KeyF5 => self.on_next_sort(),
                _ => (),
            }
        }
    }
}

// The main loop of the curses user interface.
pub fn main_loop(trace: trace::Trace, report_perf: bool) {
    let screen = pancurses::initscr();
    let mut ui = UIState::new(trace, screen);

    while !ui.exited {
        ui.draw(report_perf);
        ui.handle_input();
    }

    pancurses::endwin();
}
