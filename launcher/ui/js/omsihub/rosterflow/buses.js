// RosterFlow Pro's tab "7 Busdisponering" (round I, NexiconDK 5 November 2026; the Rust side's
// `omsihub/rosterflow/buses.rs` and the core's `company::pro_buses`): which bus runs which block.
//
// The day: a row for every bus of the fleet - its number, size, fuel and what is in its tank, its
// service and the workshop - with its blocks on a time axis (an electric bus's charging between
// them dashed), and on top in red the blocks without a bus. The week: the buses against the days,
// the blocks of each bus a day. A block clicked opens a popup: the buses that can take it (their
// warnings), a swap with a block running then, the buses that cannot and why. "Fordel automatisk"
// gives the blocks without a bus the best bus that can take them; every act can be undone from the
// note it leaves ("Fortryd").
//
// Saved at once (`rf_bus_act`), not in RosterFlow's draft: the day plan takes it (the world drives
// the bus chosen).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { rfCmd, after, bump, say, setRf } from './state.js';
import { Icon } from './ui.js';

/** The tab's own state: the date, day or week, the filter, the block whose popup is open. */
const st = () => S.rf.buses || {};
const setB = (x) => setRf((r) => ({ buses: { ...(r.buses || {}), ...x } }));

/** A command's answer, asked again when the company, the page or what it asks changed. */
function useCmd(cmd, args, keys) {
  const gen = S.co.shell?.gen ?? 0;
  const [v, setV] = useState(null);
  useEffect(() => {
    if (!args) {
      setV(null);
      return undefined;
    }
    let gone = false;
    rfCmd(cmd, args)
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ reading: e.message }));
    return () => {
      gone = true;
    };
  }, [...keys, gen, S.rf.bump, language()]);
  return v;
}

/** An act; its note (with "Fortryd") kept for the tab to show. */
async function bAct(args, setNote) {
  let r;
  try {
    r = await rfCmd('rf_bus_act', args);
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  if (r?.edited && setNote) setNote({ text: r.say?.text || '', undo: Array.isArray(r.undo) && r.undo.length ? r.undo : null, at: Date.now() });
  bump();
  return r;
}

/** Where a minute lies on the axis (per cent). */
const at = (axis, m) => ((m - axis.from) / Math.max(1, axis.to - axis.from)) * 100;

/** Blocks laid in lanes so none overlaps another (the row "Missing a bus"). */
function lanes(blocks) {
  const ends = [];
  return blocks.map((b) => {
    let k = ends.findIndex((e) => e + 5 <= b.from);
    if (k < 0) {
      k = ends.length;
      ends.push(0);
    }
    ends[k] = b.to;
    return { ...b, lane: k };
  });
}

/** A block on the axis. */
function Block({ axis, b, lane = 0, h = 34, miss, onPick, sel }) {
  const left = at(axis, b.from);
  const w = Math.max(1.2, at(axis, b.to) - left);
  const style = `left:${left}%;width:${w}%;top:${4 + lane * (h + 4)}px;height:${h}px;--k:${b.colour || '#6c7a96'}`;
  return html`<button type="button" class=${'bd-block' + (miss ? ' miss' : '') + (b.hand && !miss ? ' hand' : '') + (sel ? ' sel' : '')} style=${style}
    title=${[`${b.label} · ${b.start}–${b.end}`, `${b.km} km`, b.why, b.how].filter(Boolean).join(' · ')} onClick=${(e) => (e.stopPropagation(), onPick(b))}>
    <b>${b.label}</b><span>${b.start}–${b.end}</span></button>`;
}

/** The popup of a block: who can take it. */
function Pick({ date, line, tour, onClose, setNote }) {
  const v = useCmd('rf_bus_pick', { date, line, tour }, [date, line, tour]);
  const w = v?.words || {};
  const act = (x) => bAct({ date, line, tour, ...x }, setNote).then((r) => r?.edited && onClose());
  return html`<div class="rf-sheetdim op-popdim" onMouseDown=${(e) => e.target === e.currentTarget && onClose()}>
    <div class="rf-help op-pop bd-pop" role="dialog" aria-modal="true">
      <div class="head"><div class="pro-intro-mark" style=${v?.colour ? `background:${v.colour}` : ''}><${Icon} name="airport_shuttle" size=${20} /></div>
        <div><b>${v?.title || tr('Reading the timetable…')}</b><span>${v ? `${v.day} · ${v.sub}` : ''}</span></div>
        <button type="button" title=${tr('Close')} onClick=${onClose}><${Icon} name="close" size=${20} /></button></div>
      <div class="op-popbody"><div class="bx">
        ${v?.reading && html`<div class="bx-sub">${v.reading}</div>`}
        ${v && !v.reading && (v.current != null || v.picked) && html`<div class="bd-now">
          ${v.current != null && html`<button type="button" class="bx-btn sm line" onClick=${() => act({ act: 'remove' })}>${w.remove}</button>`}
          ${v.picked && html`<button type="button" class="bx-btn sm line" onClick=${() => act({ act: 'reset' })}>${w.reset}</button>`}
        </div>`}
        ${(v?.can || []).map((b) => html`<div class="bx-row bd-row">
          <div class="bx-grow">
            <div class="bx-name">${tr('Bus')} ${b.number}${b.line && html` <span class="bd-tag">${tr('on the line today')}</span>`}</div>
            <div class="bd-why">${b.facts}</div>
            ${b.warns.length > 0 && html`<div class="bd-warn">${b.warns.join(' · ')}</div>`}
          </div>
          ${b.swap
            ? html`<button type="button" class="bx-btn sm blue" onClick=${() => act({ act: 'swap', with_line: b.swap.line, with_tour: b.swap.tour })}>${b.swap.text}</button>`
            : html`<button type="button" class="bx-btn sm green" onClick=${() => act({ act: 'set', bus: b.id })}>${w.set}</button>`}
        </div>`)}
        ${v && !v.reading && v.can?.length === 0 && html`<div class="bx-sub" style="margin:8px 0">${tr('No bus can take it now.')}</div>`}
        ${(v?.cannot || []).length > 0 && html`<details class="bx-cant"><summary>${v.cannot_title}</summary>
          ${v.cannot.map((b) => html`<div class="bx-row bd-row"><div class="bx-grow"><div class="bx-name">${tr('Bus')} ${b.number}</div><div class="bd-why">${b.facts} · ${b.why}</div></div></div>`)}
        </details>`}
      </div></div>
    </div>
  </div>`;
}

/** The note an act leaves, with "Fortryd". */
function Note({ note, setNote, w }) {
  useEffect(() => {
    if (!note) return undefined;
    const t = setTimeout(() => setNote(null), 9000);
    return () => clearTimeout(t);
  }, [note?.at]);
  if (!note) return null;
  return html`<div class="bd-note"><span>${note.text}</span>
    ${note.undo && html`<button type="button" class="rf-link" onClick=${() => bAct({ act: 'restore', picks: note.undo }, null).then(() => setNote(null))}>${w.undo}</button>`}
    <button type="button" class="rf-ibtn" title=${tr('Close')} onClick=${() => setNote(null)}><${Icon} name="close" size=${14} /></button></div>`;
}

/** "Fordel automatisk" and "Fordel alt igen" (the second asked first). */
function AutoButtons({ v, week, setNote }) {
  const [sure, setSure] = useState(false);
  useEffect(() => {
    if (!sure) return undefined;
    const t = setTimeout(() => setSure(false), 5000);
    return () => clearTimeout(t);
  }, [sure]);
  const w = v.words;
  return html`<span class="bd-auto">
    <button type="button" class="bx-btn sm blue" onClick=${() => bAct({ act: 'auto', date: v.date, week }, setNote)}><${Icon} name="bolt" size=${14} /> ${w.auto}</button>
    ${sure
      ? html`<button type="button" class="bx-btn sm warn" title=${w.redo_sure} onClick=${() => (setSure(false), bAct({ act: 'auto', date: v.date, week, redo: true }, setNote))}>${w.redo_sure}</button>`
      : html`<button type="button" class="bx-btn sm line" onClick=${() => setSure(true)}>${w.redo}</button>`}
  </span>`;
}

function DayView({ v, filter, onPick, sel }) {
  const axis = v.axis;
  const miss = lanes(v.missing || []);
  const missLanes = Math.max(1, ...miss.map((b) => b.lane + 1));
  const rows = (v.buses || []).filter((b) => filter === 'all' || (filter === 'workshop' ? !!b.workshop : b.kind === filter));
  const isSel = (b) => sel && sel.line === b.line && sel.tour === b.tour;
  return html`<div class="bd-day">
    <div class="bd-axis"><div></div><div class="bd-ticks">${axis.hours.map((h) => html`<span style=${`left:${at(axis, h.at)}%`}>${h.text}</span>`)}</div></div>
    <div class="bd-line bd-missrow">
      <div class="bd-name miss"><b>${v.missing_title}</b>${v.missing.length === 0 && html`<span>${tr('Every block has a bus.')}</span>`}</div>
      <div class="bd-track" style=${`height:${8 + missLanes * 38}px`}>
        ${axis.hours.map((h) => html`<i class="bd-hr" style=${`left:${at(axis, h.at)}%`}></i>`)}
        ${miss.map((b) => html`<${Block} axis=${axis} b=${b} lane=${b.lane} miss onPick=${onPick} sel=${isSel(b)} />`)}
      </div>
    </div>
    ${rows.length === 0 && html`<div class="pro-dim bd-empty">${v.words.none}</div>`}
    ${rows.map((bus) => html`<div class=${'bd-line' + (bus.workshop ? ' shop' : '')}>
      <div class="bd-name"><b>${tr('Bus')} ${bus.number}</b><span class="bd-facts">${bus.facts}</span>
        ${(bus.service || bus.workshop || bus.down) && html`<span class=${'bd-state ' + (bus.workshop || bus.down ? 'bad' : 'warn')}>${[bus.workshop, bus.down, bus.service].filter(Boolean).join(' · ')}</span>`}</div>
      <div class="bd-track">
        ${axis.hours.map((h) => html`<i class="bd-hr" style=${`left:${at(axis, h.at)}%`}></i>`)}
        ${bus.shop && html`<div class="bd-shop">${bus.shop}</div>`}
        ${bus.charging.map((g) => html`<div class="bd-charge" style=${`left:${at(axis, g.from)}%;width:${at(axis, g.to) - at(axis, g.from)}%`} title=${v.words.charging}>${v.words.charging}</div>`)}
        ${bus.blocks.map((b) => html`<${Block} axis=${axis} b=${b} onPick=${onPick} sel=${isSel(b)} />`)}
        ${bus.km && html`<span class="bd-km">${bus.km}</span>`}
      </div>
    </div>`)}
  </div>`;
}

function WeekView({ v, filter, onDay }) {
  const rows = (v.buses || []).filter((b) => filter === 'all' || (filter === 'workshop' ? b.cells.some((c) => c.key === 'workshop') : b.kind === filter));
  const cols = `220px repeat(${v.days.length}, minmax(80px, 1fr))`;
  return html`<div class="cl-grid bd-week" style=${'grid-template-columns:' + cols}>
    <div class="cl-h left">${tr('Bus')}</div>
    ${v.days.map((d) => html`<button type="button" class=${'cl-h day' + (d.today ? ' today' : '')} onClick=${() => onDay(d.date)}>${d.wd}<span>${d.day}${d.today ? ` · ${tr('today')}` : ''}</span></button>`)}
    <div class="cl-name miss"><b>${v.words.missing}</b></div>
    ${v.missing.map((m) => html`<button type="button" class=${'cl-miss ' + (m.n ? 'bad' : 'ok')} onClick=${() => onDay(m.date)}>${m.n ? html`<b>${m.n}</b>${v.words.blocks}` : '✓'}</button>`)}
    ${rows.map((bus) => html`<div class="cl-name"><b>${tr('Bus')} ${bus.number}</b><span>${bus.facts}</span></div>
      ${bus.cells.map((c) => html`<button type="button" class=${'cl-cell bd-wcell ' + c.key} onClick=${() => onDay(c.date)}>${c.text}</button>`)}`)}
  </div>`;
}

export function BusesTab() {
  const b = st();
  const date = b.date || '';
  const mode = b.mode || 'day';
  const filter = b.filter || 'all';
  const [note, setNote] = useState(null);
  const v = useCmd('rf_buses', { date, mode }, [date, mode]);
  useEffect(() => {
    if (!b.pick) return undefined;
    const key = (e) => e.key === 'Escape' && setB({ pick: null });
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [b.pick]);
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const w = v.words || {};
  const go = (d) => setB({ date: d, pick: null });
  return html`<div class="pro-page bd-page">
    <div class="rf-toolbar pro-tools cl-tools">
      <button type="button" class="rf-ibtn" title=${tr('Back')} onClick=${() => go(v.prev)}><${Icon} name="chevron_left" size=${18} /></button>
      <button type="button" class="bx-btn sm line" disabled=${v.is_today && mode === 'day'} onClick=${() => go(v.company_today)}>${w.today}</button>
      <button type="button" class="rf-ibtn" title=${tr('Forward')} onClick=${() => go(v.next)}><${Icon} name="chevron_right" size=${18} /></button>
      <span class="cl-week"><b>${v.title}</b><span>${v.day_name}</span></span>
      <span class="cl-seg">${[['day', w.day], ['week', w.week]].map(([k, l]) => html`<button type="button" class=${mode === k ? 'on' : ''} onClick=${() => setB({ mode: k, pick: null })}>${l}</button>`)}</span>
      <span class="cl-seg">${v.filters.map(([k, l]) => html`<button type="button" class=${filter === k ? 'on' : ''} onClick=${() => setB({ filter: k })}>${l}</button>`)}</span>
      <span class="grow"></span>
      ${!v.reading && html`<${AutoButtons} v=${v} week=${mode === 'week'} setNote=${setNote} />`}
    </div>
    <div class="bd-main">
      ${v.reading
        ? html`<div class="rf-reading">${v.reading}</div>`
        : mode === 'week'
          ? html`<${WeekView} v=${v} filter=${filter} onDay=${(d) => setB({ date: d, mode: 'day' })} />`
          : html`<${DayView} v=${v} filter=${filter} sel=${b.pick} onPick=${(x) => setB({ pick: { line: x.line, tour: x.tour } })} />`}
    </div>
    <${Note} note=${note} setNote=${setNote} w=${w} />
    ${b.pick && mode === 'day' && html`<${Pick} date=${v.date} line=${b.pick.line} tour=${b.pick.tour} setNote=${setNote} onClose=${() => setB({ pick: null })} />`}
  </div>`;
}
