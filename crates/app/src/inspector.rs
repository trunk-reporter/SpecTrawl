//! The inspector (spec §5): whatever is selected, in full. For a record, what it was identified as and how (method,
//! confidence, the measured numbers), any other matches, the decoder on its frequency, replay, its IQ, tags and notes.
//! For an emitter, a system, a channel or a receiver, what the snapshot knows of it and its records.
//!
//! Words on screen are the user's, not the code's: a record is a "transmission", an emitter a "frequency", claims are
//! "identified as" and "other matches"; nothing explains the ranking rule.

use std::time::SystemTime;

use api::{
    AudioMode, Claim, Command, DecoderView, EmitterView, Evidence, PlayState, PlayTarget, PlayTo, PlayView, RecordId, SignalRecord,
    Snapshot, fmt,
};
use egui::RichText;

use crate::health::{self, History};
use crate::selection::Selection;
use crate::systems_pane::lamp;
use crate::theme;

/// The inspector's own state: drafts and the replay mode.
#[derive(Default)]
pub struct Inspector {
    tag_draft: String,
    note_draft: String,
    /// The mode Play asks for; `None` lets the engine choose from the frequency.
    pub play_mode: Option<AudioMode>,
}

/// "31 s ago", "4 min ago", "2 h ago".
fn ago(t: SystemTime) -> String {
    let s = SystemTime::now().duration_since(t).map_or(0.0, |d| d.as_secs_f64());
    if s < 90.0 {
        format!("{s:.0} s ago")
    } else if s < 5400.0 {
        format!("{:.0} min ago", s / 60.0)
    } else {
        format!("{:.1} h ago", s / 3600.0)
    }
}

/// One `KEY  value` row of a two-column grid.
fn field(ui: &mut egui::Ui, key: &str, value: impl Into<egui::WidgetText>) {
    ui.label(theme::label(key));
    ui.label(value);
    ui.end_row();
}

fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([12.0, 3.0]).show(ui, add);
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(theme::label(title));
}

fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(theme::INK2));
}

fn mhz4(hz: f64) -> egui::text::LayoutJob {
    theme::value_unit(&format!("{:.4}", hz / 1e6), "MHz")
}

/// What was measured, in words: "12 syncs, 3 of 4 frames decoded", "11.6 kHz, 41 dB SNR, 4800 Bd, 4 levels".
pub fn evidence_numbers(c: &Claim) -> String {
    match &c.by {
        Evidence::SyncBank { syncs, frames_tried, frames_decoded, .. } => {
            let frames = if *frames_tried > 0 { format!(", {frames_decoded} of {frames_tried} frames decoded") } else { String::new() };
            format!("{syncs} syncs{frames}")
        }
        Evidence::Features { bandwidth_hz, snr_db, symbol_rate, fsk_levels, deviation_hz, carrier_hz, .. } => {
            let mut v = vec![format!("{:.1} kHz", bandwidth_hz / 1e3)];
            if snr_db.is_finite() {
                v.push(format!("{snr_db:.0} dB SNR"));
            }
            v.extend(symbol_rate.map(|r| format!("{r:.0} Bd")));
            v.extend(fsk_levels.map(|n| format!("{n} levels")));
            v.extend(deviation_hz.map(|d| format!("±{:.1} kHz deviation", d / 1e3)));
            v.extend(carrier_hz.map(|c| format!("carrier {:.4} MHz", c / 1e6)));
            v.join(", ")
        }
        // The decoder's own sentence already names its mode and frequency.
        Evidence::Decoder { mode, locked, summary, .. } => {
            let text = if summary.is_empty() { mode.clone() } else { summary.clone() };
            if *locked { text } else { format!("{text} (not locked)") }
        }
        Evidence::BandPlan { entry } => entry.clone(),
        Evidence::Neighbour { .. } => "from a nearby signal".into(),
        Evidence::Position(p) => format!("{p:?}"),
    }
}

/// One identification: what, by which method and how sure on the first line (the chosen one solid, the others muted),
/// then what was measured.
fn claim(ui: &mut egui::Ui, c: &Claim, chosen: bool) {
    let ink = if chosen { theme::INK } else { theme::INK2 };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&c.label).color(ink).size(14.0));
        ui.label(RichText::new(format!("by {}", c.by.kind())).color(theme::INK2));
        ui.label(theme::value_unit(&format!("{:.0}", c.confidence * 100.0), "%"));
    });
    let numbers = evidence_numbers(c);
    if !numbers.is_empty() {
        ui.indent(("claim", c.label.as_str()), |ui| {
            ui.label(RichText::new(numbers).monospace().color(theme::INK2).size(11.5));
        });
    }
}

/// A decoder's lamp, sentence and counts, then its health plots.
fn decoder(ui: &mut egui::Ui, d: &DecoderView, h: &History) {
    let (word, colour) = lamp(d.lamp);
    ui.horizontal_wrapped(|ui| {
        theme::dot(ui, colour, d.lamp != api::Lamp::Searching);
        ui.label(RichText::new(word).color(colour));
        ui.label(&d.mode);
    });
    ui.label(&d.summary);
    let tried = d.blocks_ok + d.blocks_bad;
    if tried > 0 {
        muted(ui, format!("{} of {} frames decoded ({:.0} %)", d.blocks_ok, tried, 100.0 * d.blocks_ok as f64 / tried as f64));
    }
    if !d.recent.is_empty() {
        let what = if d.mode == "POCSAG" || d.mode == "FLEX" { "Newest pages" } else { "Newest calls" };
        ui.label(theme::label(what));
        for line in d.recent.iter().rev().take(5) {
            ui.label(RichText::new(line).monospace().size(11.5).color(theme::INK2));
        }
    }
    if let Some(level) = d.level_dbfs {
        ui.horizontal(|ui| {
            ui.label(theme::label("Level"));
            ui.label(theme::value_unit(&format!("{level:.1}"), "dBFS"));
        });
    }
    if let Some(s) = h.decoders.get(&(d.source, d.emitter, d.mode.clone())) {
        health::plot(ui, "SNR", "dB", &s.snr_db, 0, None);
        health::plot(ui, "Frames ok", "%", &s.ok_pct, 0, Some((0.0, 100.0)));
        health::plot(ui, "Lag", "s", &s.lag_s, 2, None);
    }
}

/// A clickable list of records, newest first: selecting one moves the inspector to it.
fn record_links<'a>(ui: &mut egui::Ui, records: impl Iterator<Item = &'a SignalRecord>, sel: &mut Selection, max: usize) {
    let mut rows: Vec<&SignalRecord> = records.collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.t_start));
    let n = rows.len();
    for r in rows.into_iter().take(max) {
        let what = r.headline().map_or("Unknown", |c| c.label.as_str());
        let text = format!("{}  {:.4}  {}  {}", fmt::clock(r.t_start), r.center_hz() / 1e6, fmt::secs(r.duration().as_secs_f64()), what);
        if ui.selectable_label(false, RichText::new(text).monospace().size(11.5)).clicked() {
            *sel = Selection::Record(r.id);
        }
    }
    if n > max {
        muted(ui, format!("and {} older", n - max));
    }
    if n == 0 {
        muted(ui, "None recently.");
    }
}

impl Inspector {
    pub fn show(&mut self, ui: &mut egui::Ui, snap: &Snapshot, sel: &mut Selection, out: &mut Vec<Command>, h: &History) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match sel.clone() {
            Selection::None => {
                muted(ui, "Click a box on a waterfall, a row in a list, or a receiver's name to see it here.");
            }
            Selection::Record(id) => match snap.records.records.iter().find(|r| r.id == id).cloned() {
                Some(r) => self.record(ui, snap, &r, sel, out, h),
                None => muted(ui, "This transmission is no longer in the recent list. Search History for it."),
            },
            Selection::Emitter(e) => match snap.records.emitters.iter().find(|x| x.id == e).cloned() {
                Some(x) => emitter(ui, snap, &x, sel, h),
                None => muted(ui, "This frequency has not been heard recently."),
            },
            Selection::System(e) => match snap.systems.systems.iter().find(|s| s.emitter == e).cloned() {
                Some(s) => {
                    ui.label(RichText::new(&s.mode).size(15.0));
                    grid(ui, "system", |ui| {
                        for (k, v) in s.identity.fields() {
                            field(ui, k, RichText::new(v).monospace());
                        }
                        field(ui, "Control", mhz4(s.control_hz));
                        field(ui, "Last seen", format!("{} ({})", fmt::clock(s.last_seen), ago(s.last_seen)));
                    });
                    ui.label(&s.summary);
                    section(ui, "Decoder");
                    match snap.decoders.iter().find(|d| d.emitter == e) {
                        Some(d) => decoder(ui, d, h),
                        None => muted(ui, "None running."),
                    }
                    if ui.link(format!("Control channel {:.4} MHz", s.control_hz / 1e6)).clicked() {
                        *sel = Selection::Emitter(e);
                    }
                    section(ui, "Recent transmissions");
                    record_links(ui, snap.records.records.iter().filter(|r| s.records.contains(&r.id)).map(|r| &**r), sel, 12);
                }
                None => muted(ui, "This system has not been heard recently."),
            },
            Selection::Channel { plan, name } => match snap.channels.channels.iter().find(|c| c.plan == plan && c.name == name).cloned() {
                Some(c) => {
                    ui.label(RichText::new(format!("{} {}", c.plan, c.name)).size(15.0));
                    grid(ui, "channel", |ui| {
                        field(ui, "Voice", c.voice.to_string());
                        field(
                            ui,
                            "Data",
                            if c.modes.is_empty() { c.data.to_string() } else { format!("{} ({})", c.data, c.modes.join(", ")) },
                        );
                        field(ui, "Carrier only", c.carrier.to_string());
                        field(ui, "On air", fmt::secs(c.airtime_s));
                        field(ui, "Peak", theme::value_unit(&format!("{:.0}", c.peak_dbfs), "dBFS"));
                        field(ui, "Last heard", format!("{} ({})", fmt::clock(c.last_heard), ago(c.last_heard)));
                    });
                    ui.label(&c.summary);
                    section(ui, "Recent transmissions");
                    record_links(ui, snap.records.records.iter().filter(|r| c.records.contains(&r.id)).map(|r| &**r), sel, 12);
                }
                None => muted(ui, "This channel has not been heard recently."),
            },
            Selection::Receiver(id) => match snap.receivers.iter().find(|r| r.id == id) {
                Some(r) => {
                    ui.horizontal(|ui| {
                        theme::chip(ui, &r.letter.to_string(), theme::CHIP, theme::INK);
                        ui.label(RichText::new(&r.device.label).size(15.0));
                    });
                    ui.label(&r.status.text);
                    if let Some(reason) = &r.status.reason {
                        muted(ui, reason.clone());
                    }
                    grid(ui, "receiver", |ui| {
                        field(ui, "Serial", RichText::new(&r.device.serial).monospace());
                        field(ui, "Mode", format!("{:?}", r.mode));
                        field(ui, "Centre", mhz4(r.center_hz));
                        field(ui, "Rate", fmt::rate(r.sample_rate));
                        field(ui, "Buffered", format!("{} (at least {})", fmt::secs(r.ring.held_secs), fmt::secs(r.ring.guaranteed_secs)));
                        field(ui, "Buffer limit", fmt::secs(r.ring.ceiling_secs));
                        field(ui, "Sample drops", r.ring.device_gaps.to_string());
                        field(ui, "Clipping", r.ring.overloads.to_string());
                        field(ui, "Display lag", fmt::secs(r.stage.behind_secs));
                    });
                    if let Some(s) = h.receivers.get(&id) {
                        section(ui, "Health");
                        health::plot(ui, "Processing lag", "s", &s.lag_s, 2, None);
                        health::plot(ui, "Display lag", "s", &s.behind_s, 2, None);
                        health::plot(ui, "Buffered", "s", &s.held_s, 0, None);
                    }
                    section(ui, "Decoders");
                    let mine: Vec<_> = snap.decoders.iter().filter(|d| d.source == id).collect();
                    if mine.is_empty() {
                        muted(ui, "None running.");
                    }
                    for d in mine {
                        if ui.link(format!("{} {:.4} MHz", d.mode, d.center_hz / 1e6)).clicked() {
                            *sel = Selection::Emitter(d.emitter);
                        }
                    }
                }
                None => muted(ui, "That receiver is closed."),
            },
        });
    }

    fn record(&mut self, ui: &mut egui::Ui, snap: &Snapshot, r: &SignalRecord, sel: &mut Selection, out: &mut Vec<Command>, h: &History) {
        let headline = r.headline();
        ui.horizontal_wrapped(|ui| {
            let (name, ink) = match headline {
                Some(c) => (c.label.as_str(), theme::INK),
                None => ("Unknown", theme::INK2),
            };
            ui.label(RichText::new(name).size(17.0).color(ink));
            muted(ui, format!("#{}", r.id.0));
        });
        let letter = snap.receivers.iter().find(|x| x.id == r.source).map_or('–', |x| x.letter);
        grid(ui, "record", |ui| {
            field(ui, "Centre", mhz4(r.center_hz()));
            field(ui, "Width", theme::value_unit(&format!("{:.1}", r.bandwidth_hz() / 1e3), "kHz"));
            let dur = if r.open { "on air".to_string() } else { fmt::secs(r.duration().as_secs_f64()) };
            field(ui, "Duration", dur);
            field(ui, "Started", format!("{} ({})", fmt::clock(r.t_start), ago(r.t_start)));
            field(ui, "Peak", theme::value_unit(&format!("{:.1}", r.peak_dbfs), "dBFS"));
            field(ui, "SNR", theme::value_unit(&format!("{:.0}", r.max_snr_db), "dB"));
            field(ui, "Receiver", RichText::new(letter.to_string()).monospace());
            // The band plan says where it is, not what it is: a field, not an identification.
            if let Some(entry) = r.claims.iter().find_map(|c| match &c.by {
                Evidence::BandPlan { entry } => Some(entry.as_str()),
                _ => None,
            }) {
                field(ui, "Band", entry);
            }
        });
        if let Some(e) = r.emitter
            && ui.link(format!("All transmissions on {:.4} MHz", r.center_hz() / 1e6)).clicked()
        {
            *sel = Selection::Emitter(e);
        }
        self.listen(ui, snap, r, out);

        section(ui, "Identified as");
        match headline {
            Some(c) => claim(ui, c, true),
            None if r.open && r.claims.is_empty() => muted(ui, "Not identified yet."),
            None => muted(ui, "Unknown"),
        }
        let mut others: Vec<&Claim> = r
            .claims
            .iter()
            .filter(|c| !headline.is_some_and(|h| std::ptr::eq(h, *c)) && !matches!(c.by, Evidence::BandPlan { .. }))
            .collect();
        if !others.is_empty() {
            others.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
            section(ui, "Other matches");
            for c in others {
                claim(ui, c, false);
            }
        }

        if let Some(e) = r.emitter {
            let decs: Vec<_> = snap.decoders.iter().filter(|d| d.emitter == e).collect();
            if !decs.is_empty() {
                section(ui, "Decoder");
                for d in decs {
                    decoder(ui, d, h);
                }
            }
        }

        section(ui, "IQ recording");
        match (&r.iq_clip, r.slice_evicted) {
            (Some(c), _) => {
                ui.label(RichText::new(format!("{} ({})", c.path.display(), fmt::secs(c.samples as f64 / c.rate))).monospace().size(11.5))
            }
            (None, true) => ui.label(RichText::new("Deleted to free disk space.").color(theme::INK2)),
            (None, false) if r.open => ui.label(RichText::new("Written when the transmission ends.").color(theme::INK2)),
            (None, false) => ui.label(RichText::new("None").color(theme::INK2)),
        };
        if !r.slice_evicted && ui.button("Save IQ").on_hover_text("Keep this IQ recording (SigMF) so it is never deleted").clicked() {
            out.push(Command::SaveIq { record: r.id });
        }
        if let Some((text, ok)) = Self::save_line(snap, r.id) {
            ui.label(RichText::new(text).color(if ok { theme::INK2 } else { theme::ALARM }));
        }
        self.tags_and_notes(ui, r, out);
    }

    /// The newest Save IQ answer for a record, and whether it worked.
    pub fn save_line(snap: &Snapshot, id: RecordId) -> Option<(String, bool)> {
        snap.acks.iter().rev().find_map(|a| match &a.result {
            Ok(api::Outcome::SavedIq { record, path, pending }) if *record == id => {
                let when = if *pending { " when the transmission ends" } else { "" };
                Some((format!("IQ saved to {}.sigmf-data{when}", path.display()), true))
            }
            Err(e) if e.starts_with(&format!("Transmission #{} has no IQ to save", id.0)) => Some((e.clone(), false)),
            _ => None,
        })
    }

    /// What Play sends for a record.
    pub fn play_command(&self, id: RecordId) -> Command {
        Command::Play { target: PlayTarget::Record(id), mode: self.play_mode, gain: 1.0, to: PlayTo::Speaker }
    }

    /// What Save audio sends: a WAV named after the record, under the engine's audio directory.
    pub fn save_command(&self, id: RecordId) -> Command {
        let to = PlayTo::Wav(format!("record-{}.wav", id.0).into());
        Command::Play { target: PlayTarget::Record(id), mode: self.play_mode, gain: 1.0, to }
    }

    /// The record's newest replay: running ones first, else the latest that ended.
    pub fn play_of(snap: &Snapshot, id: RecordId) -> Option<&PlayView> {
        let mine = || snap.audio.plays.iter().filter(move |p| p.target == PlayTarget::Record(id));
        mine().rev().find(|p| !p.state.is_over()).or_else(|| mine().next_back())
    }

    /// The line shown for a replay.
    pub fn play_line(p: &PlayView) -> String {
        let mode = format!("{}{}", p.mode.name(), if p.mode_guessed { " (auto)" } else { "" });
        let what = match (&p.to, &p.state) {
            (PlayTo::Wav(path), PlayState::Finished) => format!("saved {}", path.display()),
            (PlayTo::Wav(_), s) if !s.is_over() => "saving".into(),
            (_, PlayState::Preparing) => "starting".into(),
            (_, PlayState::Playing) => "playing".into(),
            (_, PlayState::Finished) => "played".into(),
            (_, PlayState::Stopped) => "stopped".into(),
            (_, PlayState::EndedEarly(why)) => format!("ended early: {why}"),
        };
        let length = if p.complete { fmt::secs(p.length_s) } else { format!("{}…", fmt::secs(p.length_s)) };
        format!("{what} · {} of {length} · {mode}", fmt::secs(p.position_s))
    }

    fn listen(&mut self, ui: &mut egui::Ui, snap: &Snapshot, r: &SignalRecord, out: &mut Vec<Command>) {
        ui.horizontal_wrapped(|ui| {
            let modes = [(None, "Auto"), (Some(AudioMode::Am), "AM"), (Some(AudioMode::Nfm), "NFM"), (Some(AudioMode::Wfm), "WFM")];
            let current = modes.iter().find(|m| m.0 == self.play_mode).map_or("Auto", |m| m.1);
            egui::ComboBox::from_id_salt("play-mode").selected_text(current).width(64.0).show_ui(ui, |ui| {
                for (m, name) in modes {
                    ui.selectable_value(&mut self.play_mode, m, name);
                }
            });
            let running = Self::play_of(snap, r.id).filter(|p| !p.state.is_over());
            match running {
                Some(p) if p.to == PlayTo::Speaker => {
                    if ui.button("■ Stop").clicked() {
                        out.push(Command::StopPlay(p.id));
                    }
                }
                _ => {
                    if ui.button("▶ Play").clicked() {
                        out.push(self.play_command(r.id));
                    }
                }
            }
            if ui.button("Save audio").on_hover_text("Save the audio as a WAV file").clicked() {
                out.push(self.save_command(r.id));
            }
        });
        if let Some(p) = Self::play_of(snap, r.id) {
            let playing = matches!(p.state, PlayState::Playing) && p.to == PlayTo::Speaker;
            let colour = match &p.state {
                PlayState::EndedEarly(_) => theme::ALARM,
                _ if playing => theme::LIVE,
                _ => theme::INK2,
            };
            ui.label(RichText::new(Self::play_line(p)).color(colour));
        }
        if let Err(e) = &snap.audio.output {
            muted(ui, format!("No sound: {e}"));
        }
        // A refused play is answered with the reason.
        let refused = snap.acks.iter().rev().find_map(|a| match &a.result {
            Err(e) if e.starts_with(&format!("Transmission #{} can't be played", r.id.0)) => Some(e.clone()),
            _ => None,
        });
        if let Some(e) = refused {
            ui.label(RichText::new(e).color(theme::ALARM));
        }
    }

    fn tags_and_notes(&mut self, ui: &mut egui::Ui, r: &SignalRecord, out: &mut Vec<Command>) {
        section(ui, "Tags and notes");
        ui.horizontal_wrapped(|ui| {
            for t in &r.tags {
                if ui.small_button(format!("{t} ×")).on_hover_text("Remove this tag").clicked() {
                    out.push(Command::Untag { record: r.id, tag: t.clone() });
                }
            }
            let resp = ui.add(egui::TextEdit::singleline(&mut self.tag_draft).hint_text("add a tag").desired_width(120.0));
            if (ui.small_button("Add").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                && !self.tag_draft.trim().is_empty()
            {
                out.push(Command::Tag { record: r.id, tag: self.tag_draft.trim().to_string() });
                self.tag_draft.clear();
            }
        });
        for n in &r.notes {
            ui.label(format!("{} · {}", fmt::clock(n.time), n.text));
        }
        ui.horizontal(|ui| {
            let resp = ui.add(egui::TextEdit::singleline(&mut self.note_draft).hint_text("add a note").desired_width(220.0));
            if (ui.small_button("Note").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                && !self.note_draft.trim().is_empty()
            {
                out.push(Command::Note { record: r.id, text: self.note_draft.trim().to_string() });
                self.note_draft.clear();
            }
        });
    }
}

/// An emitter (a "frequency" on screen): what it is, its numbers, its decoder and its transmissions.
fn emitter(ui: &mut egui::Ui, snap: &Snapshot, e: &EmitterView, sel: &mut Selection, h: &History) {
    ui.horizontal_wrapped(|ui| {
        let (name, ink) = match &e.headline {
            Some(c) => (c.label.as_str(), theme::INK),
            None => ("Unknown", theme::INK2),
        };
        ui.label(RichText::new(name).size(17.0).color(ink));
    });
    if let Some(c) = &e.headline {
        muted(ui, format!("by {}: {}", c.by.kind(), evidence_numbers(c)));
    }
    let stale = SystemTime::now().duration_since(e.last_seen).is_ok_and(|d| d.as_secs() > 60);
    grid(ui, "emitter", |ui| {
        field(ui, "Centre", mhz4(e.center_hz));
        field(ui, "Width", theme::value_unit(&format!("{:.1}", e.bandwidth_hz / 1e3), "kHz"));
        field(ui, "Transmissions", e.records.to_string());
        field(ui, "On air", fmt::secs(e.on_time_s));
        field(ui, "Duty", theme::value_unit(&format!("{:.0}", e.duty * 100.0), "%"));
        field(ui, "First seen", format!("{} ({})", fmt::clock(e.first_seen), ago(e.first_seen)));
        let last = RichText::new(format!("{} ({})", fmt::clock(e.last_seen), ago(e.last_seen)));
        field(ui, "Last seen", if stale { last.color(theme::INK2) } else { last });
    });
    let decs: Vec<_> = snap.decoders.iter().filter(|d| d.emitter == e.id).collect();
    if !decs.is_empty() {
        section(ui, "Decoder");
        for d in decs {
            decoder(ui, d, h);
        }
    }
    if snap.systems.systems.iter().any(|s| s.emitter == e.id) && ui.link("Trunked system").clicked() {
        *sel = Selection::System(e.id);
    }
    section(ui, "Recent transmissions");
    record_links(ui, snap.records.records.iter().filter(|r| r.emitter == Some(e.id)).map(|r| &**r), sel, 12);
}
