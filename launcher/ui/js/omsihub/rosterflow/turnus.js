// RosterFlow Pro's tab "3 Turnus" (step 3, NexiconDK 4 November 2026; the Rust side's
// `omsihub/rosterflow/turnus.rs` and the core's `company::pro_turnus`), HASTUS's way:
//
// * The rotas on the left ("Nord fuldtid", "Reserve" ...), the rules the chosen one keeps over
//   its whole cycle, the duties' colours.
// * The rota: several weeks in a row ("Turnusuge" 1 ... N), each Sunday to Saturday (the bus
//   company's week begins on Sunday), each day a duty with its times, a day off or a reserve's
//   frame; the week's hours, its marks, its weekend (its Saturday and the next week's Sunday: off
//   or at work - the drivers of an ordinary rota work every other weekend), and the driver in it in
//   the week shown - every week each driver moves a week down, after the last week he begins again
//   at week 1. A day clicked: the day's duties to put there (those no rota has first; an ordinary
//   rota no split duties, a split-duty rota only those and its weekends off), a day off, a frame.
//   A driver's place clicked: the drivers who can begin in that week.
// * "New rota": a name, its kind (ordinary, split duties, reserve) and its weeks (as many as the
//   week's duties need, suggested), made empty or filled at once (NexiconDK, 4 November 2026,
//   after the player's comments).
// * A driver's next weeks, a day of them changed for that date only (a day off, a holiday, ill, a
//   duty swapped); the week's duties no rota has.
//
// A rota is saved at once (`rf_turnus_act`), not in RosterFlow's draft. (Round C, NexiconDK 5
// November 2026: "↶ Fortryd" / "↷ Gentag" and Ctrl+Z / Ctrl+Y undo and redo the tab's edits, step
// by step - the planner's own, kept beside the company's session; a driver's day "Ill" or "On
// holiday" is his illness or holiday of the staff card; a duty swapped changes both drivers.)

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { call } from '../../api.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, after, bump, say } from './state.js';
import { Icon, RfButton, Stepper } from './ui.js';

/** Escape: the "New rota" window closed first. */
export function turnusEscape() {
  if (!st().making) return false;
  setT({ making: null });
  return true;
}

/** What the tab keeps: the rota shown, the week shown, a day or a driver's place chosen, whose
 * next weeks are shown and which of their days is chosen. */
const st = () => S.rf.turnus || {};
const setT = (patch) => setRf((r) => ({ turnus: { ...(r.turnus || {}), ...(typeof patch === 'function' ? patch(r.turnus || {}) : patch) } }));

/** An action of the tab (`rf_turnus_act`). */
async function tAct(act, args = {}) {
  let r;
  try {
    r = await rfCmd('rf_turnus_act', { act, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  if (r?.sel != null) setT({ sel: r.sel, cell: null, place: null });
  bump();
  return r;
}

/** The tab's view (`rf_turnus`), asked again when what it shows or the company changed. */
function useTurnus() {
  const t = st();
  const gen = S.co.shell?.gen ?? 0;
  const [v, setV] = useState(null);
  useEffect(() => {
    let gone = false;
    rfCmd('rf_turnus', { sel: t.sel ?? null, week: t.week || 0, driver: t.driver ?? null })
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ reading: e.message }));
    return () => {
      gone = true;
    };
  }, [t.sel, t.week, t.driver, gen, S.rf.bump, language()]);
  return v;
}

/** The colours of the duties (their signature), as the cells have them. */
const KINDS = [
  ['early', 'Early shift'],
  ['day', 'Middle shift'],
  ['late', 'Late shift'],
  ['split', 'Split shift'],
  ['reserve', 'Reserve'],
];

/** Ctrl+Z undoes the tab's last edit, Ctrl+Y (or Ctrl+Shift+Z) makes it again - not while typing,
 * nor with a window open over the tab. */
function useTurnusKeys() {
  useEffect(() => {
    const k = (e) => {
      if (!(e.ctrlKey || e.metaKey) || S.co.popup || S.co.dialog || st().making) return;
      const el = document.activeElement;
      if (el && (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || el.isContentEditable)) return;
      const key = e.key.toLowerCase();
      if (key === 'z' && !e.shiftKey) tAct('undo');
      else if (key === 'y' || (key === 'z' && e.shiftKey)) tAct('redo');
      else return;
      e.preventDefault();
    };
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  }, []);
}

export function TurnusTab() {
  useTurnusKeys();
  const v = useTurnus();
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.reading || v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const rota = v.rota;
  return html`<div class="pro-page tu-page">
    <${Toolbar} v=${v} rota=${rota} />
    <div class="pro-main">
      <${Side} v=${v} rota=${rota} />
      <div class="pro-content">
        ${v.unsaved && html`<div class="tu-note"><${Icon} name="warning" size=${15} />${tr('RosterFlow has changes not saved: the rota and the days run the saved duties. Save them to plan with them here.')}</div>`}
        ${rota ? html`<${Rota} v=${v} rota=${rota} />` : html`<div class="pro-card tu-empty">${tr('No rota yet. "New rota" on the left makes one: an ordinary rota, one of split duties or one of reserves, as many weeks long as it needs.')}</div>`}
        <div class="tu-below">
          ${v.person ? html`<${Person} v=${v} />` : html`<div class="pro-card tu-person"><div class="pro-caps">${tr('A driver\'s next weeks')}</div><div class="pro-dim">${tr('Click a driver in the rota to see his next weeks and change a day of them for that date only.')}</div></div>`}
          <${Coverage} v=${v} />
        </div>
      </div>
    </div>
    ${st().making && html`<${NewRota} v=${v} />`}
  </div>`;
}

function Toolbar({ v, rota }) {
  const t = st();
  const print = async () => {
    const r = await rfCmd('rf_print', { turnus: rota.id, week: t.week || 0, day: S.rf.day }).catch((e) => ({ say: { text: e.message, error: true } }));
    after(r);
    if (r?.printed && r.reveal) call('open_path', { path: r.folder }).catch(() => {});
  };
  const row = t.cell?.row ?? t.place;
  const h = v.history || {};
  return html`<div class="rf-toolbar pro-tools">
    <span class="pair tu-history">
      <button type="button" class="rf-ibtn tu-undo" disabled=${!h.undo?.can} title=${(h.undo?.title || '') + ' (Ctrl+Z)'} onClick=${() => tAct('undo')}><${Icon} name="livery_undo" size=${17} /><span>${tr('Undo')}</span></button>
      <button type="button" class="rf-ibtn tu-undo" disabled=${!h.redo?.can} title=${(h.redo?.title || '') + ' (Ctrl+Y)'} onClick=${() => tAct('redo')}><${Icon} name="livery_redo" size=${17} /><span>${tr('Redo')}</span></button>
    </span>
    <i class="pro-sep"></i>
    <span class="pair">
      <${RfButton} label="Make the rota automatically" solid disabled=${!rota} title=${!rota ? undefined : rota.kind === 'split' ? 'Spread the split duties of Monday to Friday no other rota has over enough weeks: the weekends off' : rota.kind === 'reserve' ? "A week for each reserve driver, five days in his frame" : "Spread the week's duties no other rota has over enough weeks: every other weekend off, two days off together, the rest kept, the hours evened"} onClick=${() => tAct('auto', { id: rota.id })} />
      <${RfButton} label="Add week" disabled=${!rota} title="A week more at the end of the rota: days off to fill" onClick=${() => tAct('add_row', { id: rota.id })} />
      <${RfButton} label="Remove week" disabled=${!rota || row == null} title="The week chosen taken out of the rota" onClick=${() => tAct('remove_row', { id: rota.id, row }).then(() => setT({ cell: null, place: null }))} />
    </span>
    <i class="pro-sep"></i>
    <button type="button" class="rf-link" disabled=${!rota} title=${tr('The rota as a PDF file, to print and hang up')} onClick=${() => rota && print()}><${Icon} name="description" size=${17} /><span>${tr('Print rota')}</span></button>
    <span class="grow"></span>
    <span class="tu-week">
      <button type="button" class="rf-ibtn" disabled=${!(t.week > 0)} title=${tr('The week before')} onClick=${() => setT({ week: Math.max(0, (t.week || 0) - 1) })}><${Icon} name="chevron_left" size=${17} /></button>
      <b>${v.week.label}</b><span>${tr('from %{day}', { day: v.week.from })}</span>
      <button type="button" class="rf-ibtn" title=${tr('The week after')} onClick=${() => setT({ week: (t.week || 0) + 1 })}><${Icon} name="chevron_right" size=${17} /></button>
    </span>
  </div>`;
}

function Side({ v, rota }) {
  const t = st();
  const [name, setName] = useState(null);
  return html`<aside class="pro-side">
    <div class="pro-caps">${tr('Rotas')}</div>
    ${v.rotas.map((x) => html`<button type="button" class=${'tu-rota' + (x.id === v.sel ? ' on' : '')} onClick=${() => setT({ sel: x.id, cell: null, place: null })}>
      <b>${x.name}</b>
      <span>${tr('%{n} weeks · %{d} drivers', { n: x.rows, d: x.drivers })} · ${x.kind === 'reserve' ? tr('reserves') : x.kind === 'split' ? tr('split duties') : tr('%{h} h a week', { h: x.hours.replace('.', ',') })}</span>
    </button>`)}
    <div class="tu-new">
      <${RfButton} label="New rota" solid=${!v.rotas.length} onClick=${() => setT({ making: { kind: 'normal' } })} />
    </div>
    ${rota && html`
      <div class="pro-caps" style="margin-top:14px">${tr('Rules over the whole cycle')}</div>
      ${rota.rules.map((r) => html`<div class=${'tu-rule' + (r.ok ? '' : ' no')}><${Icon} name=${r.ok ? 'check' : 'warning'} size=${14} /><span>${r.text}</span></div>`)}
      ${rota.general.map((g) => html`<div class="pro-dim t-warn">${g}</div>`)}
      <div class="pro-caps" style="margin-top:14px">${tr('The rota')}</div>
      <div class="pro-rule"><span>${tr('Name')}</span><input class="tu-name" value=${name ?? rota.name} onInput=${(e) => setName(e.target.value)} onBlur=${() => name != null && name !== rota.name && tAct('rename', { id: rota.id, name }).then(() => setName(null))} onKeyDown=${(e) => e.key === 'Enter' && e.target.blur()} /></div>
      <div class="pro-rule"><span>${tr('Kind')}</span><b>${rota.kind_label}</b></div>
      <div class="pro-rule"><span>${tr('Hours a week aimed at')}</span><${Stepper} value=${rota.hours ?? Number(rota.target)} step=${0.5} min=${4} max=${60} fmt=${(h) => `${String(h).replace('.', ',')} t`} onChange=${(h) => tAct('hours', { id: rota.id, hours: h })} /></div>
      ${rota.hours != null && html`<button type="button" class="rf-link tu-small" onClick=${() => tAct('hours', { id: rota.id, hours: null })}>${tr('The contract week again')}</button>`}
      <div class="pro-rule"><span>${tr('Begins')}</span><b>${rota.from_label}</b></div>
      ${(t.week || 0) > 0 && html`<button type="button" class="rf-link tu-small" title=${tr('The driver of rota week 1 is in rota week 1 in the week shown')} onClick=${() => tAct('from', { id: rota.id, week: t.week || 0 })}>${tr('Begin it in the week shown')}</button>`}
      <div class="pro-dim">${tr('Average %{a} h a week · a cycle of %{n} weeks', { a: rota.average.replace('.', ','), n: rota.cycle })}</div>
      <button type="button" class="rf-link tu-small tu-del" onClick=${() => tAct('remove', { id: rota.id }).then(() => setT({ sel: null, cell: null, place: null, driver: null }))}><${Icon} name="delete" size=${15} /><span>${tr('Delete the rota')}</span></button>
    `}
    <div class="pro-caps" style="margin-top:14px">${tr('Colours')}</div>
    <div class="tu-legend">${KINDS.map(([k, label]) => html`<span class=${'tu-chip ' + k}>${tr(label)}</span>`)}</div>
  </aside>`;
}

/** A cell of the rota or of a driver's weeks. */
function CellBox({ c, on, onClick, title, we }) {
  const mark = c.level ? html`<i class=${'tu-mark ' + c.level}>!</i>` : null;
  // (round B: the driver's wish for the day off - ★)
  const star = c.wish ? html`<i class="tu-star" title=${c.wish}>★</i>` : null;
  const tip = [title ?? c.tip, c.wish].filter(Boolean).join(' · ') || undefined;
  return html`<button type="button" class=${'tu-cell ' + (c.cls || 'off') + (on ? ' on' : '') + (c.exception ? ' ex' : '') + (we ? ' we' : '')} title=${tip} onClick=${onClick}>
    <b>${c.label}</b>${c.time && html`<span>${c.time}</span>`}${c.note && html`<span class="tu-ex">${c.note}</span>`}${mark}${star}
  </button>`;
}

function Rota({ v, rota }) {
  const t = st();
  const n = rota.rows.length;
  const head = `${rota.name} · ${rota.kind_label} · ${tr('a cycle of %{n} weeks', { n })} · ${v.week.label}`;
  const weekend = (d) => d === 0 || d === 6;
  const locked = (d) => rota.locked_weekend && weekend(d);
  return html`<div class="pro-caps head">${head}${!rota.started ? ` · ${tr('begins %{w}', { w: rota.from_label })}` : ''}</div>
    <div class="tu-explain"><${Icon} name="autorenew" size=${16} /><span>${tr('A rota is several weeks in a row. Every week each driver moves a week down, and after the last week he begins again at week 1.')} ${rota.kind === 'split' ? tr('Split duties Monday to Friday: Saturday and Sunday are always off.') : rota.kind === 'normal' ? tr('A weekend is the Saturday of one week and the Sunday of the next: the drivers work every other weekend.') : ''}</span></div>
    <div class="pro-table tu-table">
      <div class="tu-grid tu-head"><span>${tr('Rota week')}</span>${v.days.map((d, k) => html`<span class=${weekend(k) ? 'tu-we' : ''}>${d}</span>`)}<span title=${tr("The weekend after the week: its Saturday and the next week's Sunday")}>${tr('Weekend')}</span><span>${tr('Hours')}</span><span>${tr('Rules')}</span><span>${v.week.label}</span></div>
      ${n === 0 && html`<div class="pro-dim tu-none">${tr('No weeks yet: "Make the rota automatically" makes them, or add a week and fill its days.')}</div>`}
      ${rota.rows.map((row, r) => html`<div class=${'tu-grid tu-row' + (t.cell?.row === r || t.place === r ? ' on' : '')}>
        <span class="tu-no">${r + 1}</span>
        ${row.cells.map((c, d) =>
          locked(d)
            ? html`<span class="tu-cell off tu-lock" title=${tr('A split-duty rota is off on Saturday and Sunday.')}><b>${c.label}</b><${Icon} name="lock" size=${12} /></span>`
            : html`<${CellBox} c=${c} we=${weekend(d)} on=${t.cell?.row === r && t.cell?.day === d} onClick=${() => setT({ cell: t.cell?.row === r && t.cell?.day === d ? null : { row: r, day: d }, place: null })} />`,
        )}
        <${WeekendBox} w=${rota.weekends?.[r]} kind=${rota.kind} />
        <span class="tu-hours">${row.hours.replace('.', ',')}</span>
        <span class="tu-ok" title=${row.tip || tr('All rules kept')}>${row.level ? html`<b class=${row.level === 'bad' ? 't-err' : 't-warn'}>!</b>` : html`<span class="t-ok"><${Icon} name="check" size=${15} /></span>`}</span>
        <button type="button" class=${'tu-driver' + (t.place === r ? ' on' : '')} title=${tr('Who is in this rota week in the week shown; click to choose who begins in it')} onClick=${() => setT({ place: t.place === r ? null : r, cell: null })}>
          ${row.driver ? html`<b>${row.driver.name}</b><span>chf. ${row.driver.nr}</span>` : html`<span class="pro-dim">${tr('no driver')}</span>`}
        </button>
      </div>`)}
    </div>
    ${t.cell && rota.rows[t.cell.row] && html`<${CellCard} v=${v} rota=${rota} at=${t.cell} />`}
    ${t.place != null && rota.rows[t.place] && html`<${PlaceCard} v=${v} rota=${rota} row=${t.place} />`}`;
}

/** What a day of a row can be: the day's duties (those no rota has first), a day off, a frame. */
function CellCard({ v, rota, at }) {
  const set = (cell) => tAct('cell', { id: rota.id, row: at.row, day: at.day, cell });
  const now = rota.rows[at.row].cells[at.day];
  // (an ordinary rota takes no split duty, a split-duty rota only split duties)
  const all = v.duties[at.day] || [];
  const fits = (d) => (rota.kind === 'split' ? d.split : rota.kind === 'normal' ? !d.split : true);
  const duties = all.filter(fits).sort((a, b) => (a.rota == null ? 0 : 1) - (b.rota == null ? 0 : 1) || a.n - b.n);
  const splits = rota.kind === 'normal' ? all.filter((d) => d.split).length : 0;
  return html`<div class="pro-card tu-pick">
    <div class="pro-caps">${tr('Rota week %{r} · %{day}', { r: at.row + 1, day: v.days[at.day] })} · ${now.label} ${now.time || ''}</div>
    ${now.tip && html`<div class="t-err" style="white-space:pre-line;margin-bottom:6px">${now.tip}</div>`}
    <div class="tu-chips">
      <button type="button" class="tu-cell off" onClick=${() => set({ kind: 'off' })}><b>${tr('Day off')}</b></button>
      ${duties.map((d) => {
        const here = d.rota === rota.id && d.row === at.row;
        const taken = d.rota != null && !here;
        return html`<button type="button" class=${'tu-cell ' + d.cls + (taken ? ' taken' : '') + (here ? ' on' : '')} title=${taken ? tr('In %{where}', { where: d.where }) : tr('%{h} h paid', { h: d.hours.replace('.', ',') })} onClick=${() => !taken && set({ kind: 'duty', n: d.n })}><b>${d.n}</b><span>${d.time}</span></button>`;
      })}
      ${v.frames.map((f) => html`<button type="button" class="tu-cell reserve" title=${tr('At the company\'s disposal in these hours: the first to cover an open duty in them')} onClick=${() => set({ kind: 'frame', from: f.from, to: f.to })}><b>${tr('Reserve')}</b><span>${f.time}</span></button>`)}
    </div>
    <div class="pro-dim" style="margin-top:6px">${tr('The duties no rota has come first; a duty in another week that day is greyed (its place on hover).')}${splits ? ` ${tr('%{n} split duties of the day go in a split-duty rota.', { n: splits })}` : ''}</div>
  </div>`;
}

/** The weekend after a week: its Saturday and the next week's Sunday - off, at work or half. */
function WeekendBox({ w, kind }) {
  if (!w) return html`<span></span>`;
  const mark = w.level ? html`<i class=${'tu-mark ' + w.level}>!</i>` : null;
  return html`<span class=${'tu-wkend ' + w.state + (kind === 'normal' ? '' : ' quiet')} title=${`${w.label} · ${w.tip}`}><b>${w.short}</b>${mark}</span>`;
}

/** Who can go on a row: the rota's kind of drivers. */
function PlaceCard({ v, rota, row }) {
  const r = rota.rows[row];
  const fit = v.drivers.filter((d) => d.fits);
  return html`<div class="pro-card tu-pick">
    <div class="pro-caps">${tr('Rota week %{r}: who begins in it', { r: row + 1 })}${r.start ? ` · ${r.start.name}` : ''}</div>
    <div class="tu-chips">
      ${fit.map((d) => html`<button type="button" class=${'tu-person-chip' + (r.start?.id === d.id ? ' on' : '')} title=${d.where ? tr('Now on %{where}', { where: d.where }) : ''} onClick=${() => tAct('driver', { id: rota.id, row, driver: d.id })}>
        <b>${d.name}</b><span>chf. ${d.nr} · ${d.kind}${d.hours ? ` ${d.hours.replace('.', ',')} t` : ''}${d.where ? ` · ${d.where}` : ''}</span>
      </button>`)}
      ${r.start && html`<button type="button" class="tu-person-chip" onClick=${() => tAct('driver', { id: rota.id, row, driver: null })}><b>${tr('No driver')}</b><span>${tr('the week stays in the rota')}</span></button>`}
    </div>
    <div class="pro-dim" style="margin-top:6px">${rota.reserve ? tr('A reserve rota is for the reserve drivers.') : tr('Full- and part-time drivers go in a rota; the reserves in a reserve rota, the hourly paid in none.')}</div>
    ${r.start && html`<button type="button" class="rf-link tu-small" onClick=${() => setT({ driver: r.start.id })}>${tr('Show %{name}\'s next weeks', { name: r.start.name })}</button>`}
    ${r.driver && r.driver.id !== r.start?.id && html`<button type="button" class="rf-link tu-small" onClick=${() => setT({ driver: r.driver.id })}>${tr('Show %{name}\'s next weeks', { name: r.driver.name })}</button>`}
  </div>`;
}

/** A driver's next four weeks, a day of them changed for that date only. */
function Person({ v }) {
  const t = st();
  const p = v.person;
  const day = t.day && p.weeks.flatMap((w) => w.days).find((d) => d.date === t.day);
  const ex = (cell, note) => tAct('exception', { driver: p.id, date: day.date, cell, note });
  return html`<div class="pro-card tu-person">
    <div class="pro-caps">${tr('%{name} the next weeks', { name: p.name })}${p.where ? ` · ${p.where}` : ''}
      <button type="button" class="rf-link tu-x" title=${tr('Close')} onClick=${() => setT({ driver: null, day: null })}>×</button></div>
    <div class="tu-grid tu-weeks tu-head"><span></span>${v.days.map((d) => html`<span>${d}</span>`)}</div>
    ${p.weeks.map((w) => html`<div class="tu-grid tu-weeks">
      <span class="tu-wk">${w.label}</span>
      ${w.days.map((c) => html`<${CellBox} c=${c} on=${t.day === c.date} title=${`${c.short}${c.note ? ` · ${c.note}` : ''}`} onClick=${() => c.kind !== 'none' && setT({ day: t.day === c.date ? null : c.date })} />`)}
    </div>`)}
    ${day && html`<div class="tu-exc">
      <div class="pro-caps">${tr('%{date} only: the rota stays as it is', { date: day.short })}</div>
      <div class="tu-chips">
        ${day.exception && html`<button type="button" class="tu-cell" onClick=${() => tAct('exception', { driver: p.id, date: day.date, clear: true })}><b>${tr('As in the rota')}</b></button>`}
        ${v.notes.map((n) => html`<button type="button" class=${'tu-cell off' + (day.note_key === n.key && day.kind === 'off' ? ' on' : '')} onClick=${() => ex({ kind: 'off' }, n.key)}><b>${n.label}</b></button>`).filter((_, k) => v.notes[k].key !== 'swap')}
        ${(v.duties[day.day] || []).map((d) => html`<button type="button" class=${'tu-cell ' + d.cls + (day.kind === 'duty' && day.n === d.n ? ' on' : '')} title=${d.where ? tr('In %{where}: swap with its driver that day too', { where: d.where }) : ''} onClick=${() => ex({ kind: 'duty', n: d.n }, 'swap')}><b>${d.n}</b><span>${d.time}</span></button>`)}
      </div>
    </div>`}
  </div>`;
}

/** The week's duties and how many of them a rota has. */
function Coverage({ v }) {
  const c = v.coverage;
  const open = c.open.map((ns, d) => (ns.length ? html`<div><b>${v.days[d]}</b> ${ns.join(', ')}</div>` : null)).filter(Boolean);
  return html`<div class="pro-card tu-cover">
    <div class="pro-caps">${tr('Coverage')}</div>
    <div class="tu-big">${tr('%{a} of %{b}', { a: c.placed, b: c.total })}</div>
    <div class="pro-dim">${tr('duties of the week are in a rota')}</div>
    ${open.length ? html`<div class="pro-caps" style="margin-top:8px">${tr('Not in a rota')}</div><div class="tu-open">${open}</div><div class="pro-dim">${tr('They run as the roster has them, or the dispatcher fills them.')}</div>` : html`<div class="t-ok" style="margin-top:6px">${tr('Every duty of the week is in a rota.')}</div>`}
    ${c.split_open > 0 && html`<div class="tu-split">
      <div>${tr('%{n} split duties belong in a split-duty rota (Monday to Friday, the weekends off).', { n: c.split_open })}</div>
      ${!c.split_rota && html`<${RfButton} label="Make a split-duty rota" style="margin-top:6px" onClick=${() => setT({ making: { kind: 'split' } })} />`}
    </div>`}
  </div>`;
}

/** "New rota": its name, its kind and its weeks (suggested from the week's duties), made empty or
 * filled at once. */
function NewRota({ v }) {
  const t = st();
  const kinds = v.kinds || [];
  const [kind, setKind] = useState(t.making?.kind || 'normal');
  const k = kinds.find((x) => x.key === kind) || kinds[0];
  const [name, setName] = useState(null);
  const [weeks, setWeeks] = useState(null);
  const n = weeks ?? Math.max(k?.weeks || 0, 1);
  const close = () => setT({ making: null });
  const make = (fill) =>
    tAct('new', { kind, name: name ?? k?.name ?? '', weeks: n, fill }).then((r) => r && !r.say?.error && close());
  if (!k) return null;
  return html`<div class="rf-sheetdim" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="rf-help tu-newrota">
      <div class="head"><span class="pro-intro-mark"><${Icon} name="autorenew" size=${20} /></span><div><b>${tr('New rota')}</b><span>${tr('A rota is several weeks in a row: every week each driver moves a week down.')}</span></div>
        <button type="button" title=${tr('Close')} onClick=${close}><${Icon} name="close" size=${20} /></button></div>
      <div class="tu-newbody">
        <div class="pro-rule"><span>${tr('Name')}</span><input class="tu-name wide" value=${name ?? k.name} onInput=${(e) => setName(e.target.value)} /></div>
        <div class="pro-caps" style="margin-top:12px">${tr('Kind')}</div>
        <div class="tu-kinds">${kinds.map((x) => html`<button type="button" class=${'tu-kind' + (x.key === kind ? ' on' : '')} onClick=${() => (setKind(x.key), setWeeks(null), setName(null))}><b>${x.label}</b><span>${x.note}</span></button>`)}</div>
        <div class="pro-rule" style="margin-top:12px"><span>${tr('Weeks in the rota')}</span><${Stepper} value=${n} step=${1} min=${1} max=${104} fmt=${(w) => tr('%{n} weeks', { n: w })} onChange=${setWeeks} /></div>
        <div class="pro-dim">${k.weeks ? tr('Suggested from the week\'s duties no rota has: %{n} weeks.', { n: k.weeks }) : kind === 'split' ? tr('The week has no split duty from Monday to Friday that no rota has.') : tr('Every duty of the week is in a rota already.')} ${tr('%{d} drivers have no rota yet.', { d: k.drivers })}</div>
      </div>
      <div class="pro-intro-foot">
        <${RfButton} label="Cancel" style="height:32px" onClick=${close} />
        <${RfButton} label="Make it empty" style="height:32px" onClick=${() => make(false)} />
        <${RfButton} label="Make it and fill it automatically" solid style="height:32px" onClick=${() => make(true)} />
      </div>
    </div>
  </div>`;
}
