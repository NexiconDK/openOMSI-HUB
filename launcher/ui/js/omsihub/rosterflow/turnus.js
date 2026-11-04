// RosterFlow Pro's tab "3 Turnus" (step 3, NexiconDK 4 November 2026; the Rust side's
// `omsihub/rosterflow/turnus.rs` and the core's `company::pro_turnus`), HASTUS's way:
//
// * The rotas on the left ("Nord fuldtid", "Reserve" ...), the rules the chosen one keeps over
//   its whole cycle, the duties' colours.
// * The rota: a row a week, Sunday to Saturday (the bus company's week begins on Sunday), each
//   day a duty with its times, a day off or a reserve's frame; the row's hours, its marks, and
//   the driver on it in the week shown - every week each driver moves a row down, from the last
//   row to the first. A day clicked: the day's duties to put there (those no rota has first), a
//   day off, a frame. A driver's place clicked: the drivers who can go on the row.
// * A driver's next weeks, a day of them changed for that date only (a day off, a holiday, ill, a
//   duty swapped); the week's duties no rota has.
//
// A rota is saved at once (`rf_turnus_act`), not in RosterFlow's draft.

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { call } from '../../api.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, after, bump, say } from './state.js';
import { Icon, RfButton, Stepper } from './ui.js';

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

export function TurnusTab() {
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
        ${rota ? html`<${Rota} v=${v} rota=${rota} />` : html`<div class="pro-card tu-empty">${tr('No rota yet. Make one on the left: a rota of full- and part-time drivers, or of reserves.')}</div>`}
        <div class="tu-below">
          ${v.person ? html`<${Person} v=${v} />` : html`<div class="pro-card tu-person"><div class="pro-caps">${tr('A driver\'s next weeks')}</div><div class="pro-dim">${tr('Click a driver in the rota to see his next weeks and change a day of them for that date only.')}</div></div>`}
          <${Coverage} v=${v} />
        </div>
      </div>
    </div>
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
  return html`<div class="rf-toolbar pro-tools">
    <span class="pair">
      <${RfButton} label="Make the rota automatically" solid disabled=${!rota} title="Spread the week's duties no other rota has over enough rows: two days off together, the rest kept, the hours evened" onClick=${() => tAct('auto', { id: rota.id })} />
      <${RfButton} label="New row" disabled=${!rota} title="A row more at the bottom: a week of days off to fill" onClick=${() => tAct('add_row', { id: rota.id })} />
      <${RfButton} label="Delete row" disabled=${!rota || row == null} title="The row chosen deleted" onClick=${() => tAct('remove_row', { id: rota.id, row }).then(() => setT({ cell: null, place: null }))} />
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
      <span>${tr('%{n} rows · %{d} drivers', { n: x.rows, d: x.drivers })} · ${x.reserve ? tr('reserves') : tr('%{h} h a week', { h: x.hours.replace('.', ',') })}</span>
    </button>`)}
    <div class="tu-new">
      <${RfButton} label="New rota" onClick=${() => tAct('new', { reserve: false })} />
      <${RfButton} label="New reserve rota" onClick=${() => tAct('new', { reserve: true })} />
    </div>
    ${rota && html`
      <div class="pro-caps" style="margin-top:14px">${tr('Rules over the whole cycle')}</div>
      ${rota.rules.map((r) => html`<div class=${'tu-rule' + (r.ok ? '' : ' no')}><${Icon} name=${r.ok ? 'check' : 'warning'} size=${14} /><span>${r.text}</span></div>`)}
      ${rota.general.map((g) => html`<div class="pro-dim t-warn">${g}</div>`)}
      <div class="pro-caps" style="margin-top:14px">${tr('The rota')}</div>
      <div class="pro-rule"><span>${tr('Name')}</span><input class="tu-name" value=${name ?? rota.name} onInput=${(e) => setName(e.target.value)} onBlur=${() => name != null && name !== rota.name && tAct('rename', { id: rota.id, name }).then(() => setName(null))} onKeyDown=${(e) => e.key === 'Enter' && e.target.blur()} /></div>
      <div class="pro-rule"><span>${tr('Hours a row aims at')}</span><${Stepper} value=${rota.hours ?? Number(rota.target)} step=${0.5} min=${4} max=${60} fmt=${(h) => `${String(h).replace('.', ',')} t`} onChange=${(h) => tAct('hours', { id: rota.id, hours: h })} /></div>
      ${rota.hours != null && html`<button type="button" class="rf-link tu-small" onClick=${() => tAct('hours', { id: rota.id, hours: null })}>${tr('The contract week again')}</button>`}
      <div class="pro-rule"><span>${tr('Begins')}</span><b>${rota.from_label}</b></div>
      ${(t.week || 0) > 0 && html`<button type="button" class="rf-link tu-small" title=${tr('Row 1\'s driver is on row 1 in the week shown')} onClick=${() => tAct('from', { id: rota.id, week: t.week || 0 })}>${tr('Begin it in the week shown')}</button>`}
      <div class="pro-dim">${tr('Average %{a} h a week · a cycle of %{n} weeks', { a: rota.average.replace('.', ','), n: rota.cycle })}</div>
      <button type="button" class="rf-link tu-small tu-del" onClick=${() => tAct('remove', { id: rota.id }).then(() => setT({ sel: null, cell: null, place: null, driver: null }))}><${Icon} name="delete" size=${15} /><span>${tr('Delete the rota')}</span></button>
    `}
    <div class="pro-caps" style="margin-top:14px">${tr('Colours')}</div>
    <div class="tu-legend">${KINDS.map(([k, label]) => html`<span class=${'tu-chip ' + k}>${tr(label)}</span>`)}</div>
  </aside>`;
}

/** A cell of the rota or of a driver's weeks. */
function CellBox({ c, on, onClick, title }) {
  const mark = c.level ? html`<i class=${'tu-mark ' + c.level}>!</i>` : null;
  return html`<button type="button" class=${'tu-cell ' + (c.cls || 'off') + (on ? ' on' : '') + (c.exception ? ' ex' : '')} title=${title ?? (c.tip || undefined)} onClick=${onClick}>
    <b>${c.label}</b>${c.time && html`<span>${c.time}</span>`}${c.note && html`<span class="tu-ex">${c.note}</span>`}${mark}
  </button>`;
}

function Rota({ v, rota }) {
  const t = st();
  const head = `${rota.name} · ${tr('rows 1-%{n}', { n: rota.rows.length })} · ${v.week.label} · ${tr('every week the driver moves a row down')}`;
  return html`<div class="pro-caps head">${head}${!rota.started ? ` · ${tr('begins %{w}', { w: rota.from_label })}` : ''}</div>
    <div class="pro-table tu-table">
      <div class="tu-grid tu-head"><span>${tr('Row')}</span>${v.days.map((d) => html`<span>${d}</span>`)}<span>${tr('Hours')}</span><span>${tr('Rules')}</span><span>${v.week.label}</span></div>
      ${rota.rows.length === 0 && html`<div class="pro-dim tu-none">${tr('No rows yet: "Make the rota automatically" makes them, or add a row and fill its days.')}</div>`}
      ${rota.rows.map((row, r) => html`<div class=${'tu-grid tu-row' + (t.cell?.row === r || t.place === r ? ' on' : '')}>
        <span class="tu-no">${r + 1}</span>
        ${row.cells.map((c, d) => html`<${CellBox} c=${c} on=${t.cell?.row === r && t.cell?.day === d} onClick=${() => setT({ cell: t.cell?.row === r && t.cell?.day === d ? null : { row: r, day: d }, place: null })} />`)}
        <span class="tu-hours">${row.hours.replace('.', ',')}</span>
        <span class="tu-ok" title=${row.tip || tr('All rules kept')}>${row.level ? html`<b class=${row.level === 'bad' ? 't-err' : 't-warn'}>!</b>` : html`<span class="t-ok"><${Icon} name="check" size=${15} /></span>`}</span>
        <button type="button" class=${'tu-driver' + (t.place === r ? ' on' : '')} title=${tr('Who is on this row in the week shown; click to put a driver on it')} onClick=${() => setT({ place: t.place === r ? null : r, cell: null })}>
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
  const duties = [...(v.duties[at.day] || [])].sort((a, b) => (a.rota == null ? 0 : 1) - (b.rota == null ? 0 : 1) || a.n - b.n);
  return html`<div class="pro-card tu-pick">
    <div class="pro-caps">${tr('Row %{r} · %{day}', { r: at.row + 1, day: v.days[at.day] })} · ${now.label} ${now.time || ''}</div>
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
    <div class="pro-dim" style="margin-top:6px">${tr('The duties no rota has come first; a duty in another row that day is greyed (its place on hover).')}</div>
  </div>`;
}

/** Who can go on a row: the rota's kind of drivers. */
function PlaceCard({ v, rota, row }) {
  const r = rota.rows[row];
  const fit = v.drivers.filter((d) => d.fits);
  return html`<div class="pro-card tu-pick">
    <div class="pro-caps">${tr('Row %{r}: who begins on it', { r: row + 1 })}${r.start ? ` · ${r.start.name}` : ''}</div>
    <div class="tu-chips">
      ${fit.map((d) => html`<button type="button" class=${'tu-person-chip' + (r.start?.id === d.id ? ' on' : '')} title=${d.where ? tr('Now on %{where}', { where: d.where }) : ''} onClick=${() => tAct('driver', { id: rota.id, row, driver: d.id })}>
        <b>${d.name}</b><span>chf. ${d.nr} · ${d.kind}${d.hours ? ` ${d.hours.replace('.', ',')} t` : ''}${d.where ? ` · ${d.where}` : ''}</span>
      </button>`)}
      ${r.start && html`<button type="button" class="tu-person-chip" onClick=${() => tAct('driver', { id: rota.id, row, driver: null })}><b>${tr('No driver')}</b><span>${tr('the row stays in the rota')}</span></button>`}
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
    ${open.length ? html`<div class="pro-caps" style="margin-top:8px">${tr('Not in a rota')}</div><div class="tu-open">${open}</div><div class="pro-dim">${tr('They run as the roster has them, or the dispatcher fills them.')}</div>` : html`<div class="t-ok" style="margin-top:6px">${tr('Every duty of the week has a row.')}</div>`}
  </div>`;
}
