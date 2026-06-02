//! Reusable editable list/table UI for the table-backed config sections
//! (`[[commands]]`, `[[app_commands]]`, `[[quicklinks]]`, `[[snippets.entries]]`,
//! `[[hotkeys]]`, `[[datetime.timezones]]`).
//!
//! Each editor is a self-contained AppKit object: an "＋ Add" button at the top,
//! a column header row, then one row of editable controls per entry with a
//! trailing "Remove" button. Adding/removing rebuilds the rows in place; on Save
//! the parent reads every row's cell values back out (`read_values`) and
//! serializes them into the config structs.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DeclaredClass, MainThreadOnly};
use objc2_app_kit::{NSButton, NSPopUpButton, NSTextField, NSView};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

use super::helpers::{self, field, label, popup, popup_selection, str_field, PAD, ROW_H};

/// Column type: a free-text field, or a fixed-choice popup (e.g. `kind`).
#[derive(Clone)]
pub enum ColKind {
    Text,
    Choice(&'static [&'static str]),
}

/// One column in a list editor.
#[derive(Clone)]
pub struct ColSpec {
    pub header: &'static str,
    pub kind: ColKind,
    /// Default value for a newly added row (e.g. "open" for a kind column).
    pub default: &'static str,
    /// Relative width weight across the available row width.
    pub weight: f64,
}

impl ColSpec {
    pub fn text(header: &'static str, weight: f64) -> Self {
        Self { header, kind: ColKind::Text, default: "", weight }
    }
    pub fn choice(header: &'static str, choices: &'static [&'static str], default: &'static str, weight: f64) -> Self {
        Self { header, kind: ColKind::Choice(choices), default, weight }
    }
}

/// A single editable cell: either a text field or a popup button.
enum CellCtrl {
    Text(Retained<NSTextField>),
    Choice(Retained<NSPopUpButton>),
}

impl CellCtrl {
    fn value(&self) -> String {
        match self {
            CellCtrl::Text(f) => str_field(f),
            CellCtrl::Choice(p) => popup_selection(p),
        }
    }
    fn view(&self) -> &NSView {
        match self {
            CellCtrl::Text(f) => f,
            CellCtrl::Choice(p) => p,
        }
    }
    fn set_frame(&self, rect: NSRect) {
        match self {
            CellCtrl::Text(f) => f.setFrame(rect),
            CellCtrl::Choice(p) => p.setFrame(rect),
        }
    }
}

struct Row {
    cells: Vec<CellCtrl>,
    remove: Retained<NSButton>,
}

const REMOVE_W: f64 = 78.0;
const HEADER_H: f64 = ROW_H;
const ADD_H: f64 = ROW_H;
const ROW_GAP: f64 = 6.0;

pub struct EditorIvars {
    cols: Vec<ColSpec>,
    rows: RefCell<Vec<Row>>,
    doc: Retained<NSView>,
    width: RefCell<f64>,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcListEditor"]
    #[ivars = EditorIvars]
    pub struct ListEditor;

    impl ListEditor {
        #[unsafe(method(listAddRow:))]
        fn list_add_row(&self, _sender: Option<&AnyObject>) {
            let cols = self.ivars().cols.len();
            self.append_row(&vec![String::new(); cols], true);
            self.relayout();
        }

        #[unsafe(method(listRemoveRow:))]
        fn list_remove_row(&self, sender: Option<&AnyObject>) {
            let Some(s) = sender else { return };
            let tag: isize = unsafe { msg_send![s, tag] };
            if tag < 0 {
                return;
            }
            let idx = tag as usize;
            let mut rows = self.ivars().rows.borrow_mut();
            if idx < rows.len() {
                let row = rows.remove(idx);
                for cell in &row.cells {
                    cell.view().removeFromSuperview();
                }
                row.remove.removeFromSuperview();
            }
            drop(rows);
            self.relayout();
        }
    }
);

impl ListEditor {
    /// Build the cells/controls for one row from string values and add them to
    /// the document view (positions are set later by `relayout`).
    fn append_row(&self, values: &[String], _is_new: bool) {
        let mtm = self.mtm();
        let ivars = self.ivars();
        let mut cells: Vec<CellCtrl> = Vec::with_capacity(ivars.cols.len());
        for (i, col) in ivars.cols.iter().enumerate() {
            let val = values.get(i).map(String::as_str).unwrap_or(col.default);
            let cell = match &col.kind {
                ColKind::Text => CellCtrl::Text(field(mtm, val, 0.0, 0.0, 120.0)),
                ColKind::Choice(choices) => {
                    let sel = if val.is_empty() { col.default } else { val };
                    CellCtrl::Choice(popup(mtm, choices, sel, 0.0, 0.0, 120.0))
                }
            };
            ivars.doc.addSubview(cell.view());
            cells.push(cell);
        }
        let remove = helpers::button(
            mtm,
            "Remove",
            0.0,
            0.0,
            REMOVE_W,
            sel!(listRemoveRow:),
            self,
        );
        ivars.doc.addSubview(&remove);
        ivars.rows.borrow_mut().push(Row { cells, remove });
    }

    /// Re-position the Add button, header labels, and every row. Also resizes
    /// the document view so the enclosing scroll view shows all rows.
    fn relayout(&self) {
        let ivars = self.ivars();
        let w = *ivars.width.borrow();
        let n = ivars.rows.borrow().len();

        // Total height: add button + header + rows.
        let total_h = ADD_H + 6.0 + HEADER_H + 6.0 + (n as f64) * (ROW_H + ROW_GAP) + PAD;
        ivars.doc.setFrameSize(NSSize::new(w, total_h.max(ROW_H * 3.0)));

        // Flipped doc: y grows downward from the top.
        let mut y = 0.0;

        // (Add button is a separate subview created once in `build`; find via tag.)
        // Header labels live as tagged subviews too; we just lay rows here and
        // rely on `build` for static header/add positions relative to top.
        y += ADD_H + 6.0;
        y += HEADER_H + 6.0;

        // Column geometry (leave room for the Remove button on the right).
        let grid_w = w - PAD * 2.0 - REMOVE_W - 8.0;
        let total_weight: f64 = ivars.cols.iter().map(|c| c.weight).sum();
        let gap = 8.0;
        let gaps = gap * (ivars.cols.len().saturating_sub(1)) as f64;
        let usable = (grid_w - gaps).max(60.0);

        let rows = ivars.rows.borrow();
        for (ri, row) in rows.iter().enumerate() {
            let mut x = PAD;
            for (ci, cell) in row.cells.iter().enumerate() {
                let cw = usable * (ivars.cols[ci].weight / total_weight);
                cell.set_frame(NSRect::new(NSPoint::new(x, y), NSSize::new(cw, ROW_H)));
                x += cw + gap;
            }
            row.remove.setFrame(NSRect::new(
                NSPoint::new(w - PAD - REMOVE_W, y),
                NSSize::new(REMOVE_W, ROW_H),
            ));
            row.remove.setTag(ri as isize);
            y += ROW_H + ROW_GAP;
        }
    }

    /// Current values: one inner Vec of column strings per row.
    pub fn read_values(&self) -> Vec<Vec<String>> {
        self.ivars()
            .rows
            .borrow()
            .iter()
            .map(|r| r.cells.iter().map(|c| c.value()).collect())
            .collect()
    }
}

/// Build a list editor inside a freshly created flipped document view and return
/// (editor, doc_view). The caller wraps `doc_view` in a scroll view / section.
pub fn build_list_editor(
    mtm: MainThreadMarker,
    width: f64,
    cols: Vec<ColSpec>,
    initial: Vec<Vec<String>>,
) -> (Retained<ListEditor>, Retained<NSView>) {
    let doc = helpers::flipped_view(mtm, width, ROW_H * 4.0);

    let editor = ListEditor::alloc(mtm).set_ivars(EditorIvars {
        cols: cols.clone(),
        rows: RefCell::new(Vec::new()),
        doc: doc.clone(),
        width: RefCell::new(width),
    });
    let editor: Retained<ListEditor> = unsafe { msg_send![super(editor), init] };

    // Add button (top), wired to the parent's add handler which calls back into
    // this editor; we route Add through the editor itself for simplicity.
    let add = helpers::button(
        mtm,
        "\u{ff0b} Add",
        PAD,
        0.0,
        90.0,
        sel!(listAddRow:),
        &editor,
    );
    doc.addSubview(&add);

    // Column headers under the Add button.
    let header_y = ADD_H + 6.0;
    let grid_w = width - PAD * 2.0 - REMOVE_W - 8.0;
    let total_weight: f64 = cols.iter().map(|c| c.weight).sum();
    let gap = 8.0;
    let gaps = gap * (cols.len().saturating_sub(1)) as f64;
    let usable = (grid_w - gaps).max(60.0);
    let mut hx = PAD;
    for col in &cols {
        let cw = usable * (col.weight / total_weight);
        doc.addSubview(&label(mtm, col.header, hx, header_y, cw));
        hx += cw + gap;
    }

    // Populate initial rows.
    for vals in &initial {
        editor.append_row(vals, false);
    }
    editor.relayout();

    (editor, doc)
}
