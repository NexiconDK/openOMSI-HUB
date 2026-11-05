// RosterFlow Pro's tab "6 Kan arbejde" (round H, NexiconDK 5 November 2026; the Rust side's
// `omsihub/rosterflow/avail.rs` and the core's `company::pro_avail`): the hourly paid - and the
// players of Play together who drive - each a row, the next 14 days each a column: what they said
// they can that day ("05–15", "Hele dagen", "Kan ikke", or "–"). A day clicked opens a small box
// to change it: all day, from-to, not that day, or cleared. The planner changes every row; a
// player only his own. What a driver said is what he remembers when he is asked for a duty.
//
// Saved at once (`rf_avail_act`), not in RosterFlow's draft.

import { html, useEffect, useRef, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { rfCmd, after, bump, say } from './state.js';
import { Icon } from './ui.js';

/** The tab's answer, asked again when the company or the page changed. */
function useAvail(page) {
  const gen = S.co.shell?.gen ?? 0;
  const [v, setV] = useState(null);
  useEffect(() => {
    let gone = false;
    rfCmd('rf_avail', { page })
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ reading: e.message }));
    return () => {
      gone = true;
    };
  }, [page, gen, S.rf.bump, language()]);
  return v;
}

/** A day set or cleared. */
async function aAct(act, args) {
  let r;
  try {
    r = await rfCmd('rf_avail_act', { act, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  if (r?.say) say(r.say.text, r.say.error);
  bump();
  return r;
}

/** "5" or "0500" or "5:30" as "05:00", "05:30" (the times typed in the day's box: no clock of
 * the machine's language, AM/PM on an English one). */
function hhmm(t) {
  const s = String(t || '').trim().replace('.', ':');
  const m = s.match(/^(\d{1,2})(?::?(\d{2}))?$/);
  if (!m) return s;
  return `${m[1].padStart(2, '0')}:${m[2] || '00'}`;
}

/** The small box of a day: all day, from-to, not that day, cleared. */
function Edit({ v, row, cell, at, onClose }) {
  const w = v.words;
  const [from, setFrom] = useState(cell.from);
  const [to, setTo] = useState(cell.to);
  const day = v.days.find((d) => d.date === cell.date);
  const base = { driver: row.driver, guest: row.guest, date: cell.date };
  const done = (r) => r?.edited && onClose();
  return html`<div class="av-edit" style=${`left:${at.x}px;top:${at.y}px`} onClick=${(e) => e.stopPropagation()}>
    <div class="av-edit-head"><b>${row.name}</b><span>${day ? day.name || `${day.wd} ${day.day}` : cell.date}</span>
      <button type="button" class="rf-ibtn" title=${w.cancel} onClick=${onClose}><${Icon} name="close" size=${15} /></button></div>
    ${cell.by && html`<div class="av-by">${cell.by}</div>`}
    <div class="av-edit-row">
      <button type="button" class=${'bx-btn sm ' + (cell.key === 'all' ? 'green' : 'line')} onClick=${() => aAct('set', { ...base, kind: 'all' }).then(done)}>${w.all}</button>
      <button type="button" class=${'bx-btn sm ' + (cell.key === 'not' ? 'warn' : 'line')} onClick=${() => aAct('set', { ...base, kind: 'not' }).then(done)}>${w.not}</button>
    </div>
    <div class="av-edit-row">
      <label>${w.from} <input type="text" inputmode="numeric" maxlength="5" placeholder="05:00" value=${from} onInput=${(e) => setFrom(e.currentTarget.value)} /></label>
      <label>${w.to} <input type="text" inputmode="numeric" maxlength="5" placeholder="15:00" value=${to} onInput=${(e) => setTo(e.currentTarget.value)} /></label>
      <button type="button" class="bx-btn sm blue" onClick=${() => aAct('set', { ...base, kind: 'window', from: hhmm(from), to: hhmm(to) }).then(done)}>${w.save}</button>
    </div>
    <div class="av-edit-foot">
      <button type="button" class="rf-link" disabled=${cell.key === 'none'} onClick=${() => aAct('clear', base).then(done)}>${w.clear}</button>
      <button type="button" class="rf-link" onClick=${onClose}>${w.cancel}</button>
    </div>
  </div>`;
}

export function AvailTab() {
  const [page, setPage] = useState(0);
  const [open, setOpen] = useState(null);
  const box = useRef(null);
  const v = useAvail(page);
  useEffect(() => setOpen(null), [page]);
  useEffect(() => {
    if (!open) return undefined;
    const key = (e) => e.key === 'Escape' && setOpen(null);
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [open]);
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.reading || v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const pick = (e, row, cell) => {
    if (!row.may) {
      say(v.words.only_own, true);
      return;
    }
    const r = e.currentTarget.getBoundingClientRect();
    const b = box.current.getBoundingClientRect();
    const x = Math.min(Math.max(4, r.left - b.left - 60), Math.max(4, b.width - 290));
    setOpen({ driver: row.driver, guest: row.guest, date: cell.date, at: { x, y: r.bottom - b.top + 4 } });
  };
  const cols = `200px repeat(${v.days.length}, minmax(58px, 1fr))`;
  const cur = open && v.rows.find((x) => x.driver === open.driver && x.guest === open.guest);
  const cell = cur && cur.cells.find((c) => c.date === open.date);
  return html`<div class="pro-page av-page">
    <div class="rf-toolbar pro-tools cl-tools">
      <span class="cl-week"><b>${v.title}</b></span>
      <span class="cl-seg">${v.pages.map((l, k) => html`<button type="button" class=${page === k ? 'on' : ''} onClick=${() => setPage(k)}>${l}</button>`)}</span>
      <span class="grow"></span>
      <span class="av-legend">${v.legend.map(([k, l]) => html`<span><i class=${'av-cell ' + k}></i>${l}</span>`)}</span>
    </div>
    <div class="av-lead">${v.lead}</div>
    <div class="av-main" ref=${box} onClick=${() => setOpen(null)}>
      ${v.rows.length === 0
        ? html`<div class="pro-dim av-empty">${v.empty}</div>`
        : html`<div class="av-grid" style=${'grid-template-columns:' + cols}>
          <div class="cl-h left">${tr('Driver')}</div>
          ${v.days.map((d) => html`<div class=${'cl-h day' + (d.today ? ' today' : '') + (d.weekend ? ' weekend' : '')}>${d.wd}<br /><span>${d.day}${d.today ? ` · ${tr('today')}` : ''}</span></div>`)}
          ${v.rows.map((row) => html`<div class=${'cl-name' + (row.mine ? ' mine' : '')}><b>${row.name}</b><span>${row.sub}</span><span class="av-count">${row.count}</span></div>
            ${row.cells.map((c) => html`<button type="button" class=${'av-cell ' + c.key + (c.planner ? ' planner' : '') + (row.may ? '' : ' ro') + (open && open.driver === row.driver && open.guest === row.guest && open.date === c.date ? ' sel' : '')}
              title=${[c.text, c.by].filter(Boolean).join(' · ')} onClick=${(e) => (e.stopPropagation(), pick(e, row, c))}>${c.text}</button>`)}`)}
        </div>`}
      ${cur && cell && html`<${Edit} v=${v} row=${cur} cell=${cell} at=${open.at} onClose=${() => setOpen(null)} />`}
    </div>
  </div>`;
}
