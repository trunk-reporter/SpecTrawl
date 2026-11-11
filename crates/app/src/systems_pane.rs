//! SYSTEMS, the thing a trunking decoder's domain cares about (spec §5), and the decoders' lamps. Every field is the
//! snapshot's: a system row is its emitter's decoded claim, so it reads what the record and the emitter read.

use api::{DecoderView, Lamp, Snapshot, fmt};
use egui::RichText;

use crate::selection::Selection;
use crate::theme;

/// The lamp's word and colour.
pub fn lamp(l: Lamp) -> (&'static str, egui::Color32) {
    match l {
        Lamp::Searching => ("Searching", theme::INK2),
        Lamp::Syncing => ("Syncing", theme::INFO),
        Lamp::Locked => ("Locked", theme::OK),
    }
}

/// One decoder: its lamp and its sentence.
pub fn decoder_line(ui: &mut egui::Ui, d: &DecoderView) {
    let (word, colour) = lamp(d.lamp);
    ui.horizontal(|ui| {
        theme::dot(ui, colour, d.lamp != Lamp::Searching);
        ui.label(RichText::new(word).color(colour).small());
        ui.label(&d.summary);
    });
}

pub fn show(ui: &mut egui::Ui, snap: &Snapshot, sel: &mut Selection) {
    ui.label(theme::label("Systems"));
    if snap.systems.systems.is_empty() {
        ui.label(RichText::new("None yet.").italics().color(theme::INK2));
    }
    egui::Grid::new("systems").striped(true).show(ui, |ui| {
        if !snap.systems.systems.is_empty() {
            for h in ["Mode", "Control", "Identity", "Transmissions", "Last seen"] {
                ui.label(theme::label(h));
            }
            ui.end_row();
        }
        for s in &snap.systems.systems {
            let me = Selection::System(s.emitter);
            if ui.selectable_label(*sel == me, &s.mode).on_hover_text("Show details").clicked() {
                sel.toggle(me);
            }
            ui.label(fmt::mhz(s.control_hz));
            ui.label(RichText::new(s.identity.line()).strong());
            ui.label(s.records.len().to_string());
            ui.label(fmt::clock(s.last_seen));
            ui.end_row();
        }
    });
    ui.add_space(8.0);
    ui.label(theme::label("Decoders"));
    if snap.decoders.is_empty() {
        ui.label(RichText::new("None running.").italics().color(theme::INK2));
    }
    for d in &snap.decoders {
        let me = Selection::Emitter(d.emitter);
        let resp = ui.scope(|ui| decoder_line(ui, d)).response.interact(egui::Sense::click());
        if resp.on_hover_text("Show details").clicked() {
            sel.toggle(me);
        }
    }
}

/// The classify pool's budget in one line (spec §4.5: a number on screen).
pub fn budget_line(snap: &Snapshot) -> String {
    let c = &snap.classify;
    if c.workers == 0 {
        return "Identification off".into();
    }
    let load = if c.looked_secs > 0.0 { 100.0 * c.work_secs / c.looked_secs } else { 0.0 };
    let skipped = c.missed + c.dropped;
    let skipped = if skipped > 0 { format!(", {skipped} skipped") } else { String::new() };
    format!("Identification: {} checked, {} queued{skipped} · {load:.0}% of {} CPU threads", c.looked, c.waiting, c.workers)
}
