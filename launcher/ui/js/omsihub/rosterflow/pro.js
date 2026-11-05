// RosterFlow Pro's tabs "1 Vognløb" and "2 Vagter" (step 2, NexiconDK 4 November 2026; the Rust
// side's `omsihub/rosterflow/pro.rs` and the core's `company::pro_duties`), HASTUS's way:
//
// * Vognløb: every bus's block of the day on a time axis - its trips in the line's colour, the
//   runs without passengers hatched, a red mark where a driver can change (a relief point) - the
//   lines and the relief points beside it, the block chosen in words under it.
// * Vagter: the blocks cut at the relief points (a mark clicked cuts there, a cut clicked joins
//   the two pieces again), the pieces without a duty beside them (the pool), and the duties: their
//   number, type, sign-on and sign-off, work and pay, their pieces on the time axis and the marks
//   of the rules they break. A piece of the pool is dragged onto a duty (or chosen, then "Put in
//   duty"), or made a duty of its own; a duty's piece is taken out again, a duty taken apart.
//   "Make duties automatically" is RosterFlow's Optimise of the duties alone.
//
// Every edit is RosterFlow's (`rf_act`): in the draft, with Undo, until SAVE.

import { html } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { tr } from '../bits.js';
import { setRf, rfAct, edit, VEHICLES, DRIVERS } from './state.js';
import { Icon, RfButton, Stepper } from './ui.js';

/** The views of the Pro tabs (beside `state.js`'s ROSTER, VEHICLES, DRIVERS, TIMEPLAN, ROUTES). */
export const PRO_BLOCKS = 10;
export const PRO_DUTIES = 11;
/** (Step 3) The Turnus tab (`turnus.js`). */
export const PRO_ROTA = 12;
/** (Step 4) The Daglig drift and Kalender tabs (`ops.js`). */
export const PRO_DAILY = 13;
export const PRO_CALENDAR = 14;
/** (Round H) The Kan arbejde tab (`avail.js`). */
export const PRO_AVAIL = 15;
/** (Round I) The Busdisponering tab (`buses.js`). */
export const PRO_BUSES = 16;

/** The Pro tabs of the title bar: those built, and those of the steps to come. */
export const PRO_TABS = [
  { view: PRO_BLOCKS, n: 1, icon: 'airport_shuttle', label: 'Blocks', anchor: 'rf-tab-pro-blocks' },
  { view: PRO_DUTIES, n: 2, icon: 'badge', label: 'Duties', anchor: 'rf-tab-pro-duties' },
  { view: PRO_ROTA, n: 3, icon: 'autorenew', label: 'Rota', anchor: 'rf-tab-pro-rota' },
  { view: PRO_DAILY, n: 4, icon: 'today', label: 'Daily operations', anchor: 'rf-tab-pro-daily' },
  { view: PRO_CALENDAR, n: 5, icon: 'event', label: 'Calendar', anchor: 'rf-tab-pro-calendar' },
  { view: PRO_AVAIL, n: 6, icon: 'rf_can', label: 'Can work', anchor: 'rf-tab-pro-avail' },
  { view: PRO_BUSES, n: 7, icon: 'departure_board', label: 'Bus dispatch', anchor: 'rf-tab-pro-buses' },
];

/** "07:05" on the clock (minutes of the service day). */
export function clock(m) {
  const x = ((Math.round(m) % 1440) + 1440) % 1440;
  return `${String(Math.floor(x / 60)).padStart(2, '0')}:${String(x % 60).padStart(2, '0')}`;
}

/** "8:10" (a length in minutes). */
export function hm(m) {
  const x = Math.max(0, Math.round(m));
  return `${Math.floor(x / 60)}:${String(x % 60).padStart(2, '0')}`;
}

/** What is chosen on the Pro tabs: a block (Vognløb), a piece of the pool, a duty and a piece of it. */
const sel = () => S.rf.proSel || {};
const choose = (patch) => setRf((r) => ({ proSel: { ...(r.proSel || {}), ...patch } }));
const unchoose = () => setRf({ proSel: {} });

/** An edit of the Pro tabs, what was chosen let go after it (the day's numbers change). */
async function proEdit(act, args = {}) {
  const r = await edit(act, args);
  if (r?.edited) unchoose();
  return r;
}

/** Where minute `m` lies on the day's axis, in per cent. */
const at = (span, m) => ((m - span[0]) / Math.max(1, span[1] - span[0])) * 100;
const pos = (span, from, to) => `left:${at(span, from)}%;width:${Math.max(0.15, at(span, to) - at(span, from))}%`;

/** The hours of the axis, every other hour. */
function Axis({ span }) {
  const hours = [];
  for (let h = Math.ceil(span[0] / 60); h * 60 <= span[1]; h += 1) if (h % 2 === 1 || span[1] - span[0] <= 600) hours.push(h);
  return html`<div class="pro-axis">${hours.map((h) => html`<span style=${`left:${at(span, h * 60)}%`}>${String(h % 24).padStart(2, '0')}</span>`)}</div>`;
}

/** A line's chip: its number on its colour. */
const Chip = ({ number, colour }) => html`<b class="pro-chip" style=${`--c:${colour}`}>${number}</b>`;

/** The tab: Vognløb or Vagter. */
export function ProTab({ duties }) {
  const data = S.rf.data;
  if (!data?.board) return html`<div class="rf-reading">${data?.reading || tr('Reading the timetable…')}</div>`;
  const pro = data.board.pro;
  if (!pro) return html`<div class="rf-reading">${tr('RosterFlow Pro is not the planner of this company: choose it on the planning page.')}</div>`;
  return html`<div class="pro-page">
    <${Toolbar} data=${data} pro=${pro} duties=${duties} />
    <div class="pro-main">
      ${duties ? html`<${DutiesSide} pro=${pro} />` : html`<${BlocksSide} pro=${pro} />`}
      <div class="pro-content">${duties ? html`<${Duties} data=${data} pro=${pro} />` : html`<${Blocks} data=${data} pro=${pro} />`}</div>
    </div>
  </div>`;
}

function Toolbar({ data, pro, duties }) {
  const dirty = !!data.head?.draft;
  const s = sel();
  const duty = s.duty != null ? pro.duties.find((d) => d.row === s.duty) : null;
  const poolPiece = s.pool ? pro.pool.find((x) => x.tour === s.pool.tour && x.duty === s.pool.duty) : null;
  const into = duty?.pieces?.[0];
  return html`<div class="rf-toolbar pro-tools">
    <span class="pair">
      <${RfButton} label="Save" solid disabled=${!dirty} title="Make the changes the company's planning: the roster, the day and the game run it" style="width:88px" onClick=${() => rfAct('save')} />
      <${RfButton} label="Discard" disabled=${!dirty} style="width:96px" onClick=${() => rfAct('discard').then(unchoose)} />
    </span>
    <i class="pro-sep"></i>
    ${duties
      ? html`<span class="pair">
          <${RfButton} label="Make duties automatically" solid title="RosterFlow's Optimise of the duties alone: every piece in a duty, the buses' blocks kept" onClick=${() => proEdit('pro_auto')} />
          <${RfButton} label="New duty" disabled=${!poolPiece} title="The piece chosen in the pool made a duty of its own" onClick=${() => poolPiece && proEdit('pro_new', { tour: poolPiece.tour, duty: poolPiece.duty })} />
          <${RfButton} label="Put in duty" disabled=${!poolPiece || !into} title="The piece chosen in the pool put in the duty chosen (or drag it onto the duty)" onClick=${() => poolPiece && into && proEdit('pro_add', { tour: poolPiece.tour, duty: poolPiece.duty, into_tour: into.tour, into_duty: into.duty })} />
          <${RfButton} label="Take piece out" disabled=${!s.piece} title="The duty's piece chosen back to the pool" onClick=${() => s.piece && proEdit('pro_pool', s.piece)} />
          <${RfButton} label="Take duty apart" disabled=${!into} title="Every piece of the duty chosen back to the pool" onClick=${() => into && proEdit('pro_dissolve', { tour: into.tour, duty: into.duty })} />
        </span>`
      : html`<span class="pair">
          <${RfButton} label="Cut into duties" solid title="The Duties tab: cut the blocks at their relief points and build the duties" onClick=${() => setRf({ view: PRO_DUTIES, proSel: {} })} />
          <${RfButton} label="Move trips" title="RosterFlow's Vehicles tab: move trips from bus to bus on the Gantt" onClick=${() => setRf({ view: VEHICLES, proSel: {} })} />
        </span>`}
    <span class="grow"></span>
    ${duties && html`<button type="button" class="rf-link" title=${tr("RosterFlow's Drivers tab: the duties on the Gantt")} onClick=${() => setRf({ view: DRIVERS, proSel: {} })}><${Icon} name="grid_view" size=${17} /><span>${tr('Gantt')}</span></button>`}
    <span class="pro-day">${tr('Day')}: <b>${pro.day_type}</b></span>
  </div>`;
}

// --- Vognløb -------------------------------------------------------------------------------------

function BlocksSide({ pro }) {
  return html`<aside class="pro-side">
    <div class="pro-caps">${tr('Lines')}</div>
    ${pro.lines.map((l) => html`<div class="pro-lrow"><${Chip} number=${l.number} colour=${l.colour} /><span class="cut">${l.caption || l.line}</span><em>${tr('%{n} blocks', { n: l.blocks })}</em></div>`)}
    <div class="pro-caps" style="margin-top:14px">${tr('Relief points')}</div>
    ${pro.points.length ? pro.points.map((p) => html`<div class="pro-prow"><i class="pro-tri"></i><b>${p.name}</b><span>${tr('line %{n}', { n: p.lines })}</span></div>`) : html`<div class="pro-dim">${tr('No bus stands anywhere a driver can change.')}</div>`}
    <div class="pro-dim" style="margin-top:6px">${tr('The relief points are the depot, the termini and the stops added in Preferences › Relief Points.')}</div>
  </aside>`;
}

function Blocks({ data, pro }) {
  const span = data.board.span;
  const s = sel();
  const chosen = pro.blocks.find((b) => b.i === s.block);
  return html`<div class="pro-caps head">${tr('Blocks')} · ${pro.day_type} · ${tr('%{n} blocks', { n: pro.blocks.length })} · ${tr('%{n} trips', { n: pro.trips })}</div>
    <div class="pro-table">
      <div class="pro-thead"><span class="c-id">${tr('Block')}</span><span class="c-t">${tr('Pull-out')}</span><span class="c-t">${tr('Pull-in')}</span><span class="c-axis"><${Axis} span=${span} /></span></div>
      <div class="pro-rows">
        ${pro.blocks.map((b) => html`<div class=${'pro-row' + (b.i === s.block ? ' on' : '')} onClick=${() => choose({ block: b.i === s.block ? null : b.i })}>
          <span class="c-id"><${Chip} number=${b.number} colour=${b.colour} /><b>${b.label}</b></span>
          <span class="c-t">${clock(b.from)}</span><span class="c-t">${clock(b.to)}</span>
          <span class="c-axis">
            ${b.trips.map((t) => html`<i class=${'pro-trip' + (t.counts ? '' : ' dead')} style=${pos(span, t.dep, t.arr) + `;--c:${b.colour}`} title=${`${clock(t.dep)} ${t.from} – ${clock(t.arr)} ${t.to}`}>${t.counts ? (t.out ? '↗' : '↙') : ''}</i>`)}
            ${b.reliefs.map((x) => html`<i class="pro-tick" style=${`left:${at(span, x.time)}%`} title=${tr('A driver can change here, at %{stop}', { stop: x.stop })}></i>`)}
            ${b.cuts.map((x) => html`<i class="pro-tick cut" style=${`left:${at(span, x.time)}%`} title=${tr('A driver changes here, at %{stop}', { stop: x.stop })}></i>`)}
          </span>
        </div>`)}
      </div>
    </div>
    <div class="pro-legend"><i class="pro-trip sample"></i>${tr('trip with passengers')}<i class="pro-trip dead sample"></i>${tr('run to or from the depot')}<i class="pro-tri"></i><span class="t-err">${tr('relief possible')}</span></div>
    ${chosen && html`<div class="pro-card">
      <div class="pro-caps">${tr('Chosen: block %{b}', { b: chosen.label })}</div>
      <div>${clock(chosen.from)}–${clock(chosen.to)} · ${tr('%{n} trips', { n: chosen.count })} · ${chosen.km} km · ${tr('%{n} pieces', { n: chosen.pieces.length })} · ${chosen.reliefs.length ? tr('relief possible %{n} times at %{stops}', { n: chosen.reliefs.length, stops: [...new Set(chosen.reliefs.map((x) => x.stop))].join(', ') }) : tr('no relief possible')}</div>
    </div>`}`;
}

// --- Vagter --------------------------------------------------------------------------------------

function DutiesSide({ pro }) {
  const s = sel();
  const rules = pro.rules;
  const set = (k, v) => rfAct('pro_rules', { [k]: v });
  return html`<aside class="pro-side">
    <div class="pro-caps">${tr('Pieces without a duty')} · ${pro.pool.length}</div>
    ${pro.pool.length === 0 && html`<div class="pro-dim">${tr('Every piece is in a duty.')}</div>`}
    ${pro.pool.map((x) => {
      const on = s.pool && s.pool.tour === x.tour && s.pool.duty === x.duty;
      return html`<button type="button" draggable="true" class=${'pro-pool' + (on ? ' on' : '')} style=${`--c:${x.colour}`}
        title=${tr('Runs as %{d} until it is in a duty. Drag it onto a duty, or choose it and then the duty.', { d: x.runs_as })}
        onDragStart=${(e) => {
          e.dataTransfer.setData('text/plain', JSON.stringify({ tour: x.tour, duty: x.duty }));
          e.dataTransfer.effectAllowed = 'move';
        }}
        onClick=${() => choose({ pool: on ? null : { tour: x.tour, duty: x.duty }, piece: null })}>
        <${Chip} number=${x.number} colour=${x.colour} /><b>${x.label}</b><span>${clock(x.from)}–${clock(x.to)}</span>
      </button>`;
    })}
    <div class="pro-caps" style="margin-top:14px">${tr('Duty types')}</div>
    <div class="pro-types">
      <div><b>${tr('Whole')}</b> ${tr('one piece, or two with a break')}</div>
      <div><b>${tr('Split duty')}</b> ${tr('two pieces, %{t} or more unpaid between', { t: rules.split_gap })}</div>
      <div><b>${tr('Short')}</b> ${tr('under %{t} of work, paid %{t}', { t: rules.short })}</div>
    </div>
    <div class="pro-caps" style="margin-top:14px">${tr('Rules')}</div>
    <div class="pro-rule"><span>${tr('Time to sign on')}</span><${Stepper} value=${rules.sign_on} max=${60} fmt=${(v) => `${v} min`} onChange=${(v) => set('sign_on', v)} /></div>
    <div class="pro-rule"><span>${tr('Time to sign off')}</span><${Stepper} value=${rules.sign_off} max=${60} fmt=${(v) => `${v} min`} onChange=${(v) => set('sign_off', v)} /></div>
    <div class="pro-rule"><span>${tr('Change at the same stop')}</span><${Stepper} value=${rules.same_stop} max=${60} fmt=${(v) => `${v} min`} onChange=${(v) => set('same_stop', v)} /></div>
    <div class="pro-rule"><span>${tr('Walk to another stop')}</span><${Stepper} value=${rules.other_stop} max=${90} fmt=${(v) => `${v} min`} onChange=${(v) => set('other_stop', v)} /></div>
    <div class="pro-dim" style="margin-top:6px">${tr('Driving at most %{c} before a break · a split duty spread at most %{s}', { c: rules.continuous, s: rules.spread })}</div>
  </aside>`;
}

/** A piece of the pool dropped on a duty: put in it. */
function dropOn(e, duty) {
  e.preventDefault();
  let p = null;
  try {
    p = JSON.parse(e.dataTransfer.getData('text/plain'));
  } catch {
    return;
  }
  const into = duty.pieces[0];
  if (p && into) proEdit('pro_add', { tour: p.tour, duty: p.duty, into_tour: into.tour, into_duty: into.duty });
}

function Duties({ data, pro }) {
  const span = data.board.span;
  const s = sel();
  const duty = s.duty != null ? pro.duties.find((d) => d.row === s.duty) : null;
  // (the blocks of what is chosen first: a duty's, a piece's of the pool)
  const mine = new Set([...(duty?.pieces || []).map((x) => x.tour), ...(s.pool ? [s.pool.tour] : [])]);
  const blocks = [...pro.blocks].sort((a, b) => (mine.has(b.i) ? 1 : 0) - (mine.has(a.i) ? 1 : 0));
  const status = (d) => (d.level === 'ok' ? html`<span class="t-ok" title=${tr('All rules kept')}><${Icon} name="check" size=${16} /></span>` : html`<span class=${d.level === 'bad' ? 't-err' : 't-warn'} title=${d.tip}><b>!</b></span>`);
  return html`<div class="pro-caps head">${tr('Blocks cut at the relief points')} · <span class="t-err">${tr('red line = relief')}</span> · ${tr('a mark clicked cuts there, a line clicked joins again')}</div>
    <div class="pro-table cutview">
      <div class="pro-thead"><span class="c-id">${tr('Block')}</span><span class="c-axis"><${Axis} span=${span} /></span></div>
      <div class="pro-rows">
        ${blocks.map((b) => html`<div class=${'pro-row' + (mine.has(b.i) ? ' on' : '')}>
          <span class="c-id"><${Chip} number=${b.number} colour=${b.colour} /><b>${b.label}</b></span>
          <span class="c-axis">
            ${b.pieces.map((x) => html`<i class=${'pro-piece' + (x.pool ? ' pool' : '') + (s.pool && s.pool.tour === b.i && s.pool.duty === x.k ? ' on' : '')} style=${pos(span, x.from, x.to) + `;--c:${b.colour}`}
              title=${`${x.label}: ${clock(x.from)} ${x.from_stop} – ${clock(x.to)} ${x.to_stop}`}
              onClick=${() => (x.pool ? choose({ pool: { tour: b.i, duty: x.k }, piece: null }) : x.row != null && choose({ duty: x.row, pool: null, piece: { tour: b.i, duty: x.k } }))}>${x.letter} · ${x.pool ? tr('no duty') : x.number}</i>`)}
            ${b.reliefs.map((x) => html`<button type="button" class="pro-mark" style=${`left:${at(span, x.time)}%`} title=${tr('Cut here: a driver takes over at %{stop} (%{time})', { stop: x.stop, time: clock(x.time) })} onClick=${() => proEdit('pro_cut', { tour: b.i, at: x.at })}></button>`)}
            ${b.cuts.map((x) => html`<button type="button" class="pro-cut" style=${`left:${at(span, x.time)}%`} title=${tr('A driver takes over here, at %{stop}. Click to join the two pieces again.', { stop: x.stop })} onClick=${() => proEdit('uncut', { tour: b.i, at: x.at })}></button>`)}
          </span>
        </div>`)}
      </div>
    </div>
    <div class="pro-caps head">${tr('Duties')} · ${pro.duties.length} · ${pro.pool.length ? tr('every piece in a duty but %{n}', { n: pro.pool.length }) : tr('every piece in a duty')}</div>
    <div class="pro-table duties">
      <div class="pro-thead"><span class="c-n">${tr('Duty')}</span><span class="c-k">${tr('Type')}</span><span class="c-t">${tr('Signs on')}</span><span class="c-t">${tr('Signs off')}</span><span class="c-t">${tr('Work')}</span><span class="c-t">${tr('Paid')}</span><span class="c-axis"><${Axis} span=${span} /></span><span class="c-s"></span></div>
      <div class="pro-rows">
        ${pro.duties.map((d) => html`<div class=${'pro-row' + (s.duty === d.row ? ' on' : '')} onClick=${() => choose({ duty: s.duty === d.row ? null : d.row, piece: null })} onDragOver=${(e) => e.preventDefault()} onDrop=${(e) => dropOn(e, d)}>
          <span class="c-n"><b>${d.number}</b>${d.locked && html`<${Icon} name="lock" size=${12} />`}</span>
          <span class=${'c-k k-' + d.kind}>${d.kind_label}</span>
          <span class="c-t">${clock(d.start)}</span><span class="c-t">${clock(d.end)}</span>
          <span class="c-t">${hm(d.work)}</span><span class="c-t">${hm(d.paid)}</span>
          <span class="c-axis">
            <i class="pro-sign" style=${pos(span, d.start, d.end)}></i>
            ${d.pieces.map((x) => html`<i class=${'pro-piece' + (s.piece && s.piece.tour === x.tour && s.piece.duty === x.duty ? ' on' : '')} style=${pos(span, x.from, x.to) + `;--c:${x.colour}`} title=${`${x.label}: ${clock(x.from)} ${x.from_stop} – ${clock(x.to)} ${x.to_stop}`}
              onClick=${(e) => {
                e.stopPropagation();
                choose({ duty: d.row, piece: { tour: x.tour, duty: x.duty }, pool: null });
              }}>${x.label}</i>`)}
            ${d.between.map((g) => html`<i class=${'pro-gap ' + g.kind} style=${pos(span, g.from, g.to)} title=${hm(g.minutes)}>${g.kind === 'break' ? 'P' : g.kind === 'walk' ? '→' : ''}</i>`)}
          </span>
          <span class="c-s">${status(d)}</span>
        </div>`)}
      </div>
    </div>
    ${duty && html`<div class="pro-card">
      <div class="pro-caps">${tr('Chosen: duty %{d}', { d: duty.number })}${duty.line_change ? ` · ${tr('changes line')}` : ''}</div>
      <div>${duty.detail}</div>
      ${duty.tip && html`<div class=${duty.level === 'bad' ? 't-err' : 't-warn'} style="white-space:pre-line;margin-top:4px">${duty.tip}</div>`}
    </div>`}`;
}
