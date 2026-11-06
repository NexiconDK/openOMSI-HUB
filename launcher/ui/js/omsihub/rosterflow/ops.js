// RosterFlow Pro's tabs "4 Daglig drift" and "5 Kalender" (step 4, NexiconDK 4 November 2026, made
// clearer after the player's sketches the same day; the Rust side's `omsihub/rosterflow/ops.rs`
// and the core's `company::pro_ops` and `company::pro_cut`):
//
// * Kalender: the drivers grouped - permanent, reserves with a frame, hourly paid -, each day of
//   the week from Sunday his duty and its working time (a day off "–", ill or on holiday one bar
//   across the days), the colours the kind of duty, and the hours of his week against his
//   contract. Above them "Mangler en chauffør": a day clicked opens the box in a popup over the
//   calendar (the player's wish, NexiconDK 4 November 2026: the calendar keeps its whole width).
// * The box, one step at a time: 1. the day's duties without a driver, each "Find a driver ›";
//   2. who can drive the duty - permanent drivers, reserves, the hourly paid, the cheapest first -
//   each with one thing to do (put on, ask, waiting for the answer, give it after a yes) and his
//   answer in a box of its own (green yes, red no); when nobody can or will: cut the duty, its
//   parts each with one driver suggested.
//   A driver who said no today already wears it: irritated, angry (`pro_ops::Mood`).
// * Round B (NexiconDK, 5 November 2026): the drivers' wishes - "Ønsker (N)" in the toolbar, ★ on
//   their days, a note by their name in the popup -, time off in lieu - overtime paid or saved
//   when given, a day of it from the day's list, "afsp." under a name -, and what the week costs
//   above the calendar and what each driver costs extra for a duty (`pro_wish`, `pro_toil`,
//   `pro_cost`).
// * Daglig drift: the day's duties without a driver, with the day's frames and who is free; "Find a
//   driver ›" opens the same popup.
//
// Saved at once (`rf_ops_act`), not in RosterFlow's draft. (Round C, NexiconDK 5 November 2026:
// a driver on another duty then who could hand it to a free colleague can be asked - "⇄"; a swap
// changes both drivers at once, shows in both their cells and is undone in the day's box -
// "Fortryd bytte" - until its first duty begins.) Round E (NexiconDK, 5 November 2026): the drivers
// propose swaps themselves - "Bytteforslag (N)", each approved or refused -, the season of the week
// in the head ("❄ Vinter – flere syge"), and the Monday reports - "Rapporter", tiles of the week
// before (`pro_trade`, `pro_season`, `pro_report`). Round G (NexiconDK, 5 November 2026): who can
// drive on one line a driver - a tick box, the name and its signs, the price, the answer beside it
// -, those ticked asked at once ("Spørg de valgte", the permanent drivers too); a mark on the day's
// "missing" cell for the answers come and not seen ("✓ 1 ja", "🕒 venter på 3", "✗ alle sagde nej"),
// and a yes said at the bottom wherever RosterFlow Pro is open (`AnswerWatch`).

import { html, useEffect, useState } from '../../../vendor/preact-htm.js';
import { S } from '../../store.js';
import { language } from '../../i18n.js';
import { tr } from '../bits.js';
import { setRf, rfCmd, after, bump, say } from './state.js';
import { Icon, RfButton } from './ui.js';

/** What the tabs keep: the calendar's week, span, filter and search; the date and the duty the
 * box shows, whether its popup is open (`pop`) and the duty just given (`given`). */
const st = () => S.rf.ops || {};
const setO = (patch) => setRf((r) => ({ ops: { ...(r.ops || {}), ...(typeof patch === 'function' ? patch(r.ops || {}) : patch) } }));

/** The popup closed. */
const closePop = () => setO({ pop: false, n: null, given: null });

/** Escape: the popup closed first (the wishes', the swap proposals', the reports' or the day's). */
export function opsEscape() {
  if (st().wishes || st().trades || st().reports) {
    setO({ wishes: false, trades: false, reports: false });
    return true;
  }
  if (!st().pop) return false;
  closePop();
  return true;
}

/** An act that left a duty open (a day off given): its who-can-drive opened in the popup. */
function openFreed(r) {
  if (r?.open) setO({ wishes: false, pop: true, date: r.open.date, n: r.open.n, given: null });
}

/** A command's answer, asked again when what it shows or the company changed. */
function useView(cmd, args, keys) {
  const gen = S.co.shell?.gen ?? 0;
  const [v, setV] = useState(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let gone = false;
    rfCmd(cmd, args)
      .then((x) => !gone && setV(x))
      .catch((e) => !gone && setV({ reading: e.message }));
    return () => {
      gone = true;
    };
  }, [...keys, gen, S.rf.bump, language(), tick]);
  return [v, () => setTick((t) => t + 1)];
}

/** An action of the box (`rf_ops_act`). */
async function oAct(act, args = {}) {
  let r;
  try {
    r = await rfCmd('rf_ops_act', { act, ...args });
  } catch (e) {
    say(e.message, true);
    return null;
  }
  after(r);
  bump();
  return r;
}

// --- the box ---------------------------------------------------------------------------------------

/** A driver's answer in a box of its own: green for yes, red for no (too early, too late), grey
 * for no answer. */
function Answer({ a }) {
  if (!a) return null;
  const cls = a.yes ? 'yes' : a.key === 'silent' ? 'silent' : 'no';
  return html`<div class=${'bx-ans ' + cls}><b>${a.chip}</b>${a.words && html`<span>${a.words}</span>`}</div>`;
}

/** The one thing to do with a driver, by the rules of asking (breaking the rest rules: "anyway",
 * asked first - `onAnyway`). */
function Action({ x, onAsk, onGive, onSet, onBack, onAnyway }) {
  switch (x.action) {
    case 'set':
      return html`<button type="button" class="bx-btn green" onClick=${onSet}>${tr('Put on')}</button>`;
    case 'set_anyway':
      return html`<button type="button" class="bx-btn warn" onClick=${onAnyway}>${tr('Put on anyway')}</button>`;
    case 'give_anyway':
      return html`<button type="button" class="bx-btn warn" onClick=${onAnyway}>${tr('Give anyway')}</button>`;
    case 'ask':
      return html`<button type="button" class="bx-btn line" title=${x.hint || ''} onClick=${onAsk}>${tr('Ask the driver')}</button>`;
    case 'done':
      return html`<span class="bx-done">${x.hint}</span>`;
    case 'waiting':
      return html`<span class="bx-wait"><span>${tr('Waiting for an answer…')}</span>${onBack && html`<button type="button" class="rf-link" onClick=${onBack}>${tr('Take back')}</button>`}</span>`;
    case 'give':
      return html`<button type="button" class="bx-btn green" onClick=${onGive}>${x.answer?.swap ? tr('Give him the duty, with the swap') : tr('Give him the duty')}</button>`;
    default:
      return null;
  }
}

/** Given: a whole duty back to the day's list, with a line saying so (a part: the cut view stays). */
async function giveIt(n, part, name, args) {
  // (round B: a driver's overtime paid or saved as time off in lieu - `args.toil`)
  const r = await oAct('give', args);
  if (r?.edited && part == null) setO({ n: null, given: tr('Duty %{n} is given to %{name}', { n, name }) });
}

/** A driver's row of the box: his name (and his mood, said no today already), kind and price, a
 * note, his answer, and the one thing. */
function Row({ v, n, part, x }) {
  const base = { date: v.date, n, part };
  const [sure, setSure] = useState(false);
  const [save, setSave] = useState(null);
  const toil = x.toil ? (save ?? x.toil.on) : null;
  const give = (anyway) => giveIt(n, part, x.name, { ...base, driver: x.id, swap: x.answer?.swap || null, anyway, toil });
  const sub = x.toil ? (toil ? x.toil.save : x.toil.pay) : x.sub;
  const giving = ['set', 'give', 'set_anyway', 'give_anyway'].includes(x.action);
  return html`<div class=${'bx-row' + (x.breach ? ' breach' : '')}>
    <div class="bx-grow">
      <div class="bx-name">${x.name}${x.mood && html` <span class=${'bx-mood ' + x.mood.key}>${x.mood.text}</span>`}${x.extra && html` <span class="bx-extra">${x.extra}</span>`}</div>
      <div class=${'bx-sub' + (x.over && !toil ? ' op-over' : '') + (toil ? ' bx-saved' : '')}>${sub}</div>
      ${x.wish && x.wish.map((w) => html`<div class="bx-wish">${w}</div>`)}
      ${x.pref && html`<div class=${'bx-pref' + (x.pref.fits > 0 ? ' fits' : x.pref.fits < 0 ? ' nofit' : '')}>${x.pref.text}</div>`}
      ${x.tired && html`<div class=${'bx-tired ' + x.tired.key}>${x.tired.text}</div>`}
      ${x.late && html`<div class="bx-late">${x.late}</div>`}
      ${x.toil && giving && html`<span class="cl-seg bx-toil">${x.toil.options.map(([k, l]) => html`<button type="button" class=${(k === 'save') === !!toil ? 'on' : ''} onClick=${() => setSave(k === 'save')}>${l}</button>`)}</span>`}
      ${x.note && html`<div class="bx-sub">${x.note}</div>`}
      ${x.offer && html`<div class="bx-swapoffer">${x.offer}</div>`}
      ${x.breach && x.breach.map((b) => html`<div class="bx-breach">${b}</div>`)}
      <${Answer} a=${x.answer} />
      ${x.swap && html`<div class="bx-swap">${x.swap.text}</div>`}
      ${sure && html`<div class="bx-sure"><span>${x.anyway}</span>
        <span class="bx-surebtns"><button type="button" class="bx-btn line" onClick=${() => setSure(false)}>${tr('Cancel')}</button>
        <button type="button" class="bx-btn warn" onClick=${() => (setSure(false), give(true))}>${tr('Yes, do it')}</button></span></div>`}
    </div>
    ${!sure && html`<${Action}
      x=${x}
      onAsk=${() => oAct('ask', { ...base, drivers: [x.id] })}
      onSet=${() => give(false)}
      onGive=${() => give(false)}
      onAnyway=${() => setSure(true)}
      onBack=${x.ask != null ? () => oAct('withdraw_ask', { ...base, ask: x.ask }) : null}
    />`}
  </div>`;
}

/** Nobody for it: an agency driver when the agency setting is on, else its trips are dropped. */
function Uncovered({ v, n, part, d }) {
  if (d.agency) return html`<button type="button" class="bx-btn line" onClick=${() => oAct('agency', { date: v.date, n, part })}>${d.agency}</button>`;
  if (d.drop) return html`<div class="bx-drop">${d.drop}</div>`;
  return null;
}

/** Step 1: the day's duties without a driver (in the popup: the duty just given, and a way out
 * when all are filled). */
function Missing({ v, pop }) {
  const given = st().given;
  return html`<div class="bx">
    ${!pop && html`<div class="bx-head"><b>${v.heading}</b><span class=${v.open.length ? 'bad' : 'ok'}>${v.count.text}</span></div>`}
    ${pop && given && html`<div class="bx-givenok"><${Icon} name="check" size=${16} /> ${given}</div>`}
    ${v.open.length === 0 &&
    (pop
      ? html`<div class="bx-allfull"><b><${Icon} name="check" size=${22} /> ${tr('All duties are filled')}</b><span>${tr('Nothing to fill on this day: every duty has a driver.')}</span>
          <button type="button" class="bx-btn blue" onClick=${closePop}>${tr('Close')}</button></div>`
      : html`<div class="bx-allok"><${Icon} name="check" size=${16} /> ${tr('Nothing to fill on this day: every duty has a driver.')}</div>`)}
    ${v.open.map((d) => html`<div class="bx-row">
      <div class="bx-grow">
        <div class="bx-name">${d.label} <span class="bx-time">${d.time}</span></div>
        <div class="bx-sub">${d.why}</div>
      </div>
      <button type="button" class="bx-btn blue" onClick=${() => setO({ date: v.date, n: d.n, pop: true, given: null })}>${tr('Find a driver ›')}</button>
    </div>`)}
    ${v.swaps?.list?.length > 0 && html`<div class="bx-group">${v.swaps.caps}</div>
      ${v.swaps.list.map((w) => html`<div class="bx-row">
        <div class="bx-grow"><div class="bx-name">${w.text}</div><div class="bx-sub">${w.duties}</div></div>
        ${w.can
          ? html`<button type="button" class="bx-btn line" onClick=${() => oAct('unswap', { date: v.date, no: w.no })}>${v.swaps.undo}</button>`
          : html`<span class="bx-sub" title=${w.hint || ''}>${w.hint}</span>`}
      </div>`)}`}
    ${v.toil?.length > 0 && html`<div class="bx-group">${v.toil_caps}</div>
      ${v.toil.map((t) => html`<div class="bx-row">
        <div class="bx-grow"><div class="bx-name">${t.name} <span class="bx-time">chf. ${t.nr}</span></div><div class=${'bx-sub' + (t.on ? ' bx-saved' : '')}>${t.what}</div></div>
        ${t.on
          ? html`<button type="button" class="rf-link" onClick=${() => oAct('toil_back', { date: v.date, driver: t.id })}>${tr('Take it back')}</button>`
          : html`<button type="button" class="bx-btn line" onClick=${() => oAct('toil_day', { date: v.date, driver: t.id }).then(openFreed)}>${t.take}</button>`}
      </div>`)}`}
  </div>`;
}

/** The first sign of a text ("🌅 Foretrækker tidlige vagter" → 🌅) - its icon in a row. */
const sign = (t) => (t ? Array.from(t.trim())[0] : '');

/** (Round G) A driver's answer in the row's own column: what he said in a word (his words on
 * hover), or how long until it comes, or "–" not asked. */
function AnswerCell({ x }) {
  const a = x.answer;
  if (x.action === 'waiting')
    return html`<span class="wl-wait">🕒 ${x.eta || tr('Waiting for an answer…')}</span>`;
  if (x.action === 'done') return html`<span class="wl-dim" title=${x.hint || ''}>${tr('Answers no more today')}</span>`;
  if (!a) return x.action === 'ask' ? html`<span class="wl-dim">–</span>` : null;
  const cls = a.swap ? 'swap' : a.yes ? 'yes' : a.key === 'silent' ? 'silent' : 'no';
  const label = a.swap ? tr('⇄ Swap') : a.chip;
  const tip = [a.words, x.swap?.text].filter(Boolean).join(' · ');
  // (a driver's own words written small under the answer, not only on hover)
  return html`<span class="wl-ansbox"><span class=${'wl-ans ' + cls} title=${tip}>${label}</span>${tip && html`<span class="wl-why">${tip}</span>`}</span>`;
}

/** (Round G) One driver of the who-can-drive list on one line: a tick box (to ask with the
 * others ticked), the name with its signs (mood, wish, early/late, tired, short notice, a swap
 * possible, the rest rules - the words on hover), the price, his answer, and the one thing. */
function Line({ v, n, x, picked, onPick }) {
  const base = { date: v.date, n };
  const [sure, setSure] = useState(false);
  const [save, setSave] = useState(null);
  const toil = x.toil ? (save ?? x.toil.on) : null;
  const give = (anyway) => giveIt(n, null, x.name, { ...base, driver: x.id, swap: x.answer?.swap || null, anyway, toil });
  const price = x.toil ? (toil ? x.toil.save_price : x.toil.pay_price) : x.price;
  const giving = ['give', 'give_anyway', 'set', 'set_anyway'].includes(x.action);
  const signs = [
    x.mood && [x.mood.key === 'angry' ? '😠' : '😒', [x.mood.text, x.hint].filter(Boolean).join(' · '), 'mood'],
    x.wish?.length && ['★', x.wish.join(' · '), 'wish'],
    x.pref && [sign(x.pref.text), x.pref.text, x.pref.fits > 0 ? 'fits' : x.pref.fits < 0 ? 'nofit' : ''],
    x.tired && [sign(x.tired.text), x.tired.text, 'tired ' + x.tired.key],
    x.late && ['⏰', x.late, 'late'],
    x.offer && ['⇄', x.offer, 'offer'],
    x.breach && ['⚠', x.breach.join(' · '), 'breach'],
    x.note && ['ⓘ', x.note, ''],
  ].filter(Boolean);
  const button = () => {
    switch (x.action) {
      case 'set':
        return html`<button type="button" class="bx-btn green sm" onClick=${() => give(false)}>${tr('Put on')}</button>`;
      case 'set_anyway':
        return html`<button type="button" class="bx-btn warn sm" title=${x.anyway || ''} onClick=${() => setSure(true)}>${tr('Put on anyway')}</button>`;
      case 'give':
        return html`<button type="button" class="bx-btn green sm" onClick=${() => give(false)}>${x.answer?.swap ? tr('Give with the swap') : tr('Give')}</button>`;
      case 'give_anyway':
        return html`<button type="button" class="bx-btn warn sm" title=${x.anyway || ''} onClick=${() => setSure(true)}>${tr('Give anyway')}</button>`;
      case 'waiting':
        return x.ask != null && html`<button type="button" class="wl-x" title=${tr('Take back')} onClick=${() => oAct('withdraw_ask', { ...base, ask: x.ask })}>✕</button>`;
      default:
        return null;
    }
  };
  return html`<div class=${'wl-row' + (x.breach ? ' breach' : '') + (picked ? ' picked' : '') + (x.action === 'cannot' ? ' cannot' : '')}>
    <span class="wl-tick">${x.action === 'ask' && html`<input type="checkbox" checked=${picked} title=${tr('Tick to ask')} onChange=${(e) => onPick(e.currentTarget.checked)} />`}</span>
    <span class="wl-who" title=${x.sub}><b>${x.name}</b><i class="wl-chip">${x.chip}</i>${x.avail && html`<i class=${'wl-avail ' + x.avail.key}>${x.avail.text}</i>`}${x.extra && html`<i class="wl-extra">${x.extra}</i>`}${signs.map(([g, t, k]) => html`<i class=${'wl-sign ' + k} title=${t}>${g}</i>`)}</span>
    <span class=${'wl-price' + (x.over && !toil ? ' op-over' : '') + (toil ? ' bx-saved' : '')} title=${x.sub}>${price}</span>
    <span class="wl-answer"><${AnswerCell} x=${x} />
      ${x.toil && giving && html`<span class="cl-seg wl-toil">${x.toil.options.map(([k, l]) => html`<button type="button" class=${(k === 'save') === !!toil ? 'on' : ''} onClick=${() => setSave(k === 'save')}>${l}</button>`)}</span>`}
      ${!sure && button()}</span>
    ${sure && html`<div class="wl-sure"><span>${x.anyway}</span>
      <span class="bx-surebtns"><button type="button" class="bx-btn line sm" onClick=${() => setSure(false)}>${tr('Cancel')}</button>
      <button type="button" class="bx-btn warn sm" onClick=${() => (setSure(false), give(true))}>${tr('Yes, do it')}</button></span></div>`}
  </div>`;
}

/** Step 2: who can drive the duty - round G (NexiconDK, 5 November 2026): one line a driver,
 * those to ask ticked and asked at once ("Spørg de valgte"), their answers beside them as they
 * come. The box opened, its answers are seen (the day's mark off them). */
function Who({ v, d }) {
  const back = () => setO({ n: null, given: null });
  const [picked, setPicked] = useState({});
  useEffect(() => setPicked({}), [v.date, d.n]);
  useEffect(() => {
    if (!d.unseen) return;
    rfCmd('rf_ops_act', { act: 'seen', date: v.date, n: d.n })
      .then((r) => r?.seen && bump())
      .catch(() => {});
  }, [v.date, d.n, d.unseen]);
  const pick = (key, on) => setPicked((p) => ({ ...p, [key]: on }));
  const rows = d.groups.flatMap((g) => g.rows);
  const drivers = rows.filter((x) => x.action === 'ask' && picked['d' + x.id]).map((x) => x.id);
  const guests = d.players.filter((p) => p.action === 'ask' && picked['g' + p.no]).map((p) => p.no);
  const count = drivers.length + guests.length;
  const askable = rows.some((x) => x.action === 'ask') || d.players.some((p) => p.action === 'ask');
  const askAll = async () => {
    const r = await oAct('ask', { date: v.date, n: d.n, drivers, guests });
    if (r?.edited) setPicked({});
  };
  return html`<div class="bx wl">
    <div class="bx-top"><button type="button" class="rf-link bx-back" onClick=${back}><${Icon} name="chevron_left" size=${16} /><span>${tr('Back')}</span></button>
      ${d.sort && html`<span class="cl-seg bx-sort">${d.sort.options.map(([k, l]) => html`<button type="button" class=${d.sort.value === k ? 'on' : ''} onClick=${() => setO({ sort: k })}>${l}</button>`)}</span>`}</div>
    ${d.given && html`<div class="bx-given"><span>${tr('given to %{name}', { name: d.given })}</span><button type="button" class="rf-link" onClick=${() => oAct('withdraw', { date: v.date, n: d.n })}>${tr('Take it back')}</button></div>`}
    ${d.groups.map((g) => html`<div class="wl-group">${g.label}</div>${g.rows.map((x) => html`<${Line} v=${v} n=${d.n} x=${x} picked=${!!picked['d' + x.id]} onPick=${(on) => pick('d' + x.id, on)} />`)}`)}
    ${d.players.length > 0 && html`<div class="wl-group">${tr('Players')}</div>
      ${d.players.map((p) => html`<div class="wl-row">
        <span class="wl-tick">${p.action === 'ask' && html`<input type="checkbox" checked=${!!picked['g' + p.no]} onChange=${(e) => pick('g' + p.no, e.currentTarget.checked)} />`}</span>
        <span class="wl-who"><b>${p.name}</b><i class="wl-chip">${tr('a player: answers himself')}</i>${p.avail && html`<i class=${'wl-avail ' + p.avail.key}>${p.avail.text}</i>`}</span>
        <span class="wl-price"></span>
        <span class="wl-answer">${p.action === 'waiting' ? html`<span class="wl-wait">🕒 ${tr('Waiting for an answer…')}</span>` : p.answer ? html`<span class="wl-ansbox"><span class=${'wl-ans ' + (p.answer.yes ? 'yes' : 'no')} title=${p.answer.words || ''}>${p.answer.chip}</span>${p.answer.words && html`<span class="wl-why">${p.answer.words}</span>`}</span>` : html`<span class="wl-dim">–</span>`}
          ${p.action === 'give' && html`<button type="button" class="bx-btn green sm" onClick=${() => oAct('give_guest', { date: v.date, n: d.n, guest: p.no }).then((r) => r?.edited && setO({ n: null, given: tr('Duty %{n} is given to %{name}', { n: d.n, name: p.name }) }))}>${tr('Give')}</button>`}</span>
      </div>`)}`}
    ${d.nobody &&
    html`<div class="bx-note"><b>${tr('Nobody can drive all of the duty.')}</b><span>${tr('Cut it in pieces, so that as much as possible is driven.')}</span></div>
      ${d.can_cut && html`<button type="button" class="bx-btn blue wide" onClick=${() => oAct('cut', { date: v.date, n: d.n })}>${tr('Cut the duty ›')}</button>`}
      <div style="margin-top:8px"><${Uncovered} v=${v} n=${d.n} d=${d} /></div>`}
    ${d.cant.length > 0 && html`<details class="bx-cant"><summary>${d.cant_label}</summary>${d.cant.map((x) => html`<div class="bx-row dim"><div class="bx-grow"><div class="bx-name">${x.name}</div><div class="bx-sub">${x.why}</div></div></div>`)}</details>`}
    ${askable && html`<div class="wl-bar"><span>${tr('%{n} ticked', { n: count })}${d.cant.length > 0 ? ` · ${d.cant_label}` : ''}</span>
      <button type="button" class="bx-btn blue" disabled=${count === 0} title=${count === 0 ? tr('Tick the drivers to ask first.') : ''} onClick=${askAll}>${tr('Ask those ticked')}</button></div>`}
  </div>`;
}

/** The cut view: the duty's parts, each with its driver or one suggested. */
function Cut({ v, k }) {
  const span = Math.max(1, k.last - k.first);
  const ticks = [k.first, ...k.parts.slice(1).map((p) => p.from), k.last];
  const clock = (m) => `${String(Math.floor(m / 60) % 24).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
  return html`<div class="bx">
    <button type="button" class="rf-link bx-back" onClick=${() => setO({ n: null, given: null })}><${Icon} name="chevron_left" size=${16} /><span>${tr('Back')}</span></button>
    <div class="bx-bar">${k.parts.map((p) => html`<span class=${p.given || p.row ? 'ok' : 'bad'} style=${`flex:${Math.max(1, p.to - p.from)}`}>${p.letter}</span>`)}</div>
    <div class="bx-ticks">${ticks.map((t) => html`<span style=${`left:${((t - k.first) / span) * 100}%`}>${clock(t)}</span>`)}</div>
    ${k.parts.map((p) => html`<div class="bx-part">
      <div class="bx-name">${p.title}</div>
      ${p.given && html`<div class="bx-sub ok">${p.given}</div>`}
      ${p.row && html`<${Row} v=${v} n=${k.n} part=${p.i} x=${p.row} />`}
      ${p.none && html`<div class="bx-row"><div class="bx-grow"><div class="bx-sub">${p.none}</div></div><${Uncovered} v=${v} n=${k.n} part=${p.i} d=${p} /></div>`}
    </div>`)}
    <div class="bx-sub" style="margin-top:10px">${k.driven}</div>
    <button type="button" class="rf-link" onClick=${() => oAct('uncut', { date: v.date, n: k.n })}>${tr('Join the duty again')}</button>
  </div>`;
}

/** The box's view (`rf_ops`) for a date and the duty chosen (none: the day's list), asked again
 * every two seconds while answers are on their way. */
function useBox(date, n) {
  const sort = st().sort || 'cheap';
  const [v, again] = useView('rf_ops', { date: date || null, n, sort }, [date, n, sort]);
  useEffect(() => {
    if (!v?.waiting) return;
    const t = setTimeout(again, 2000);
    return () => clearTimeout(t);
  }, [v]);
  return v;
}

/** The box's step shown: the day's list, who can drive the duty, or its cut. */
function Step({ v, pop }) {
  if (!v) return html`<div class="bx"><div class="rf-reading" style="position:static">${tr('Reading the timetable…')}</div></div>`;
  if (v.reading || v.off) return html`<div class="bx"><div class="pro-dim">${v.reading}</div></div>`;
  if (v.cut) return html`<${Cut} v=${v} k=${v.cut} />`;
  if (v.duty) return html`<${Who} v=${v} d=${v.duty} />`;
  return html`<${Missing} v=${v} pop=${pop} />`;
}

/** The daily operations' list of the day's duties without a driver ("Find a driver ›": the popup). */
function DayList({ date }) {
  return html`<${Step} v=${useBox(date, null)} />`;
}

/** The popup over the calendar or the daily operations, one step at a time: its title (the day,
 * the duty or its cut), the box inside; closed with ✕, Escape or a click beside it, open while
 * the answers come in. */
function Pop({ date }) {
  const v = useBox(date, st().n ?? null);
  const ok = v && !v.reading && !v.off;
  const head = !v ? [tr('Reading the timetable…'), ''] : v.cut ? [v.cut.title, v.cut.sub] : v.duty ? [v.duty.title, `${v.duty.sub} · ${v.duty.why}${v.duty.holiday ? ` · ${v.duty.holiday}` : ''}`] : ok ? [v.heading, v.count?.text] : [v.reading || '', ''];
  const bad = ok && !v.cut && !v.duty && v.open?.length > 0;
  return html`<div class="rf-sheetdim op-popdim" onMouseDown=${(e) => e.target === e.currentTarget && closePop()}>
    <div class="rf-help op-pop" role="dialog" aria-modal="true">
      <div class="head"><div class="pro-intro-mark"><${Icon} name=${v?.duty || v?.cut ? 'groups' : 'event'} size=${20} /></div>
        <div><b>${head[0]}</b><span class=${bad ? 'op-popbad' : ''}>${head[1]}</span></div>
        <button type="button" title=${tr('Close')} onClick=${closePop}><${Icon} name="close" size=${20} /></button></div>
      <div class="op-popbody"><${Step} v=${v} pop /></div>
    </div>
  </div>`;
}

/** The drivers' wishes in a popup (round B): each "Grant" or "Refuse"; a day off granted that
 * leaves the driver's duty open opens who can drive it. */
function Wishes() {
  const [v] = useView('rf_wishes', {}, []);
  const close = () => setO({ wishes: false });
  const decide = async (act, w) => {
    let r;
    try {
      r = await rfCmd('rf_wish_act', { act, wish: w.id });
    } catch (e) {
      say(e.message, true);
      return;
    }
    after(r);
    bump();
    openFreed(r);
  };
  return html`<div class="rf-sheetdim op-popdim" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="rf-help op-pop" role="dialog" aria-modal="true">
      <div class="head"><div class="pro-intro-mark"><${Icon} name="star" size=${20} /></div>
        <div><b>${v?.title || tr('Reading the timetable…')}</b><span>${v?.sub || ''}</span></div>
        <button type="button" title=${tr('Close')} onClick=${close}><${Icon} name="close" size=${20} /></button></div>
      <div class="op-popbody"><div class="bx">
        ${v?.list?.length === 0 && html`<div class="bx-allok"><${Icon} name="check" size=${16} /> ${v.sub}</div>`}
        ${(v?.list || []).map((w) => html`<div class="bx-row">
          <div class="bx-grow">
            <div class="bx-name">${w.name} <span class="bx-time">chf. ${w.nr} · ${w.chip}</span></div>
            <div class="bx-wish big">${w.text}</div>
            ${w.leaves && html`<div class="bx-breach">${w.leaves}</div>`}
            <div class="bx-sub">${w.sent}</div>
          </div>
          <span class="bx-surebtns"><button type="button" class="bx-btn line" onClick=${() => decide('refuse', w)}>${tr('Refuse')}</button>
          <button type="button" class="bx-btn green" onClick=${() => decide('grant', w)}>${tr('Grant')}</button></span>
        </div>`)}
        ${v?.note && html`<div class="bx-sub" style="margin-top:10px">${v.note}</div>`}
      </div></div>
    </div>
  </div>`;
}

/** The drivers' own swap proposals in a popup (round E): each "Approve" or "Refuse"; approved, the
 * swap of round C is made for both at once. */
function Trades() {
  const [v] = useView('rf_trades', {}, []);
  const close = () => setO({ trades: false });
  const decide = async (act, t) => {
    let r;
    try {
      r = await rfCmd('rf_trade_act', { act, trade: t.id });
    } catch (e) {
      say(e.message, true);
      return;
    }
    after(r);
    bump();
  };
  return html`<div class="rf-sheetdim op-popdim" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="rf-help op-pop" role="dialog" aria-modal="true">
      <div class="head"><div class="pro-intro-mark"><${Icon} name="rf_swap" size=${20} /></div>
        <div><b>${v?.title || tr('Reading the timetable…')}</b><span>${v?.sub || v?.reading || ''}</span></div>
        <button type="button" title=${tr('Close')} onClick=${close}><${Icon} name="close" size=${20} /></button></div>
      <div class="op-popbody"><div class="bx">
        ${v?.list?.length === 0 && html`<div class="bx-allok"><${Icon} name="check" size=${16} /> ${v.sub}</div>`}
        ${(v?.list || []).map((t) => html`<div class="bx-row">
          <div class="bx-grow">
            <div class="bx-name">${t.a} <span class="bx-time">chf. ${t.a_nr} · ${t.a_kind}</span> ⇄ ${t.b} <span class="bx-time">chf. ${t.b_nr} · ${t.b_kind}</span></div>
            <div class="bx-trade">${t.text}</div>
            <div class="bx-why">${t.why}</div>
            ${t.breach.map((b) => html`<div class="bx-breach">${b}</div>`)}
            <div class="bx-sub">${t.sent} · ${t.until}</div>
          </div>
          <span class="bx-surebtns"><button type="button" class="bx-btn line" onClick=${() => decide('refuse', t)}>${tr('Refuse')}</button>
          <button type="button" class=${'bx-btn ' + (t.breach.length ? 'warn' : 'green')} onClick=${() => decide('approve', t)}>${tr('Approve')}</button></span>
        </div>`)}
        ${v?.note && html`<div class="bx-sub" style="margin-top:10px">${v.note}</div>`}
      </div></div>
    </div>
  </div>`;
}

/** The Monday reports in a popup (round E): the newest first, as tiles - red what went badly,
 * green what went well -, arrows to the older ones. */
function Reports() {
  const k = st().rk || 0;
  const [v] = useView('rf_reports', { k }, [k]);
  const close = () => setO({ reports: false, rk: 0 });
  return html`<div class="rf-sheetdim op-popdim" onMouseDown=${(e) => e.target === e.currentTarget && close()}>
    <div class="rf-help op-pop op-report" role="dialog" aria-modal="true">
      <div class="head"><div class="pro-intro-mark"><${Icon} name="rf_report" size=${20} /></div>
        <div><b>${v?.title || tr('Reading the timetable…')}</b><span>${v?.sub || ''}</span></div>
        <button type="button" title=${tr('Close')} onClick=${close}><${Icon} name="close" size=${20} /></button></div>
      <div class="op-popbody"><div class="bx">
        ${v?.empty && html`<div class="bx-allok">${v.empty}</div>`}
        ${v?.count > 1 && html`<div class="rp-nav">
          <button type="button" class="rf-ibtn" disabled=${!v.older} title=${tr('The week before')} onClick=${() => setO({ rk: k + 1 })}><${Icon} name="chevron_left" size=${17} /></button>
          <span>${v.made}</span>
          <button type="button" class="rf-ibtn" disabled=${!v.newer} title=${tr('The week after')} onClick=${() => setO({ rk: Math.max(0, k - 1) })}><${Icon} name="chevron_right" size=${17} /></button>
        </div>`}
        <div class="rp-tiles">${(v?.tiles || []).map((t) => html`<div class=${'rp-tile ' + t.tone}><span>${t.label}</span><b>${t.value}</b><i>${t.sub}</i></div>`)}</div>
      </div></div>
    </div>
  </div>`;
}

// --- 5 Kalender --------------------------------------------------------------------------------------

const SPANS = ['week', 'two', 'month'];

/** A driver's days: a cell each, ill or on holiday one bar across the days in a row. */
function Days({ v, x, o }) {
  const out = [];
  for (let i = 0; i < x.cells.length; i++) {
    const c = x.cells[i];
    if (c.kind === 'ill' || c.kind === 'holiday' || c.kind === 'toil') {
      let j = i;
      while (j + 1 < x.cells.length && x.cells[j + 1].kind === c.kind) j++;
      const label = j > i ? `${c.label} ${v.days[i].name} – ${v.days[j].name}` : c.label;
      out.push(html`<div class=${'cl-cell bar ' + c.cls} style=${`grid-column:span ${j - i + 1}`}><b>${label}</b></div>`);
      i = j;
      continue;
    }
    const past = v.days[i].past ? ' past' : '';
    const star = c.wish && html`<i class="cl-star" title=${c.wish}>★</i>`;
    // (round C: a swap he is in that day - both drivers changed at once)
    const swap = c.swap && html`<i class="cl-swap" title=${c.swap}>⇄</i>`;
    // (round D: tired that day - 😮‍💨 / 🥱, and why)
    const tired = c.tired && html`<i class=${'cl-tired ' + c.tired.key} title=${c.tired.text}>${c.tired.key === 'worn' ? '🥱' : '😮‍💨'}</i>`;
    const tip = [c.tired?.text, c.swap, c.wish || c.tip].filter(Boolean).join(' · ');
    if (c.kind === 'off' || c.kind === 'away') {
      // (round H: an hourly-paid driver's empty day he said he can work - a faint mark, its times)
      const avail = c.avail && html`<i class="cl-avail">${c.avail_t}</i>`;
      out.push(html`<div class=${'cl-cell off' + past + (c.wish ? ' wish' : '') + (c.swap ? ' swapped' : '') + (c.avail ? ' can' : '')} title=${[c.avail, tip].filter(Boolean).join(' · ')}>${c.label === '–' ? '–' : html`<b>${c.label}</b>`}${star}${swap}${avail}</div>`);
      continue;
    }
    out.push(html`<button type="button" class=${'cl-cell ' + c.cls + past + (c.wish ? ' wish' : '') + (c.swap ? ' swapped' : '')} title=${tip} onClick=${() => setO({ date: v.days[i].date, n: null, pop: true, given: null })}>
      <b>${c.label}</b>${c.sub && html`<span>${c.sub}</span>`}${star}${swap}${tired}${c.ram && html`<i class="cl-ram" title=${tr('In the frame %{name}', { name: c.ram })}>${c.ram}</i>`}
    </button>`);
  }
  return out;
}

export function CalendarTab() {
  const o = st();
  const [v] = useView('rf_cal', { week: o.week || 0, span: o.span || 'week', kind: o.kind || 'all' }, [o.week, o.span, o.kind]);
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.reading || v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const q = (o.q || '').trim().toLowerCase();
  const list = q ? v.drivers.filter((x) => x.name.toLowerCase().includes(q) || String(x.nr) === q) : v.drivers;
  const weeks = v.drivers[0]?.hours?.length || 1;
  const many = v.days.length > 7;
  const cols = `170px repeat(${v.days.length}, minmax(${many ? 64 : 92}px, 1fr)) ${weeks > 1 ? 70 * Math.min(weeks, 5) : 128}px`;
  const day = o.date || v.days.find((d) => d.today)?.date || v.days[0]?.date;
  const groups = ['permanent', 'reserve', 'hourly'].map((g) => [g, list.filter((x) => x.group === g)]).filter((g) => g[1].length);
  return html`<div class="pro-page op-page">
    <div class="rf-toolbar pro-tools cl-tools">
      <span class="tu-week">
        <button type="button" class="rf-ibtn" disabled=${!(o.week > 0)} title=${tr('The week before')} onClick=${() => setO({ week: Math.max(0, (o.week || 0) - 1) })}><${Icon} name="chevron_left" size=${17} /></button>
        <${RfButton} label="This week" solid=${!(o.week > 0)} onClick=${() => setO({ week: 0 })} />
        <button type="button" class="rf-ibtn" title=${tr('The week after')} onClick=${() => setO({ week: (o.week || 0) + 1 })}><${Icon} name="chevron_right" size=${17} /></button>
      </span>
      <span class="cl-week"><b>${v.week.label}</b><span>${v.week.span_text} · ${v.now}</span></span>
      <span class="cl-seg">${['Week', '14 days', 'Month'].map((l, k) => html`<button type="button" class=${(o.span || 'week') === SPANS[k] ? 'on' : ''} onClick=${() => setO({ span: SPANS[k] })}>${tr(l)}</button>`)}</span>
      <span class="cl-seg">${v.kinds.map(([k, label]) => html`<button type="button" class=${(o.kind || 'all') === k ? 'on' : ''} onClick=${() => setO({ kind: k })}>${label}</button>`)}</span>
      <span class="grow"></span>
      ${v.wishes && html`<button type="button" class=${'rf-btn cl-wishes' + (v.wishes.open ? ' has' : '')} onClick=${() => setO({ wishes: true })}><${Icon} name="star" size=${16} /><span>${v.wishes.label}</span></button>`}
      ${v.trades && html`<button type="button" class=${'rf-btn cl-wishes cl-trades' + (v.trades.open ? ' has' : '')} onClick=${() => setO({ trades: true })}><${Icon} name="rf_swap" size=${16} /><span>${v.trades.label}</span></button>`}
      ${v.reports && html`<button type="button" class=${'rf-btn cl-wishes cl-reports' + (v.reports.new ? ' has' : '')} onClick=${() => setO({ reports: true, rk: 0 })}><${Icon} name="rf_report" size=${16} /><span>${v.reports.label}</span>${v.reports.new && html`<i class="cl-dot"></i>`}</button>`}
      <label class="cl-search"><${Icon} name="search" size=${16} /><input type="text" value=${o.q || ''} placeholder=${tr('Search a driver')} onInput=${(e) => setO({ q: e.currentTarget.value })} /></label>
    </div>
    ${(v.cost || v.season) && html`<div class="cl-cost" title=${v.cost?.tip || ''}>${v.cost && html`<${Icon} name="payments" size=${16} /><b>${v.cost.parts[0]}</b>${v.cost.parts.slice(1).map((x) => html`<span>· ${x}</span>`)}`}
      ${v.season && html`<span class=${'cl-season ' + v.season.key} title=${v.season.tip}>${v.season.text}</span>`}</div>`}
    <div class="cl-main">
      <div class="cl-cal">
        <div class="cl-grid" style=${'grid-template-columns:' + cols}>
          <div class="cl-h left">${tr('Driver')}</div>
          ${v.days.map((d) => html`<button type="button" class=${'cl-h day' + (d.today ? ' today' : '') + (d.past ? ' past' : '') + (d.holiday ? ' holiday' : '')} title=${d.holiday_tip || d.label} onClick=${() => setO({ date: d.date, n: null, pop: true, given: null })}>${d.wd}<br /><span>${d.day}${d.today ? ` · ${tr('today')}` : ''}</span>${d.holiday && html`<em class="cl-holiday">✦ ${d.holiday}</em>`}</button>`)}
          <div class="cl-h left">${weeks > 1 ? tr('Hours a week') : tr('Hours this week')}</div>
          <div class="cl-name miss"><b>${tr('Missing a driver')}</b><span>${tr('click to fill')}</span></div>
          ${v.days.map((d) => html`<button type="button" class=${'cl-miss' + (d.missing ? ' bad' : ' ok') + (d.date === day ? ' sel' : '')} onClick=${() => setO({ date: d.date, n: null, pop: true, given: null })}>
            ${d.missing ? html`<b>${d.missing}</b><span>${d.missing_text}</span>` : html`<${Icon} name="check" size=${16} /><span>${d.missing_text}</span>`}
            ${d.mark && html`<i class=${'cl-mark ' + d.mark.key} title=${d.mark.text} onClick=${(e) => (e.stopPropagation(), setO({ date: d.date, n: d.mark.n ?? null, pop: true, given: null }))}>${d.mark.text}</i>`}
          </button>`)}
          <div></div>
          ${groups.map(([g, xs]) => html`<div class="cl-group" style=${`grid-column:1 / span ${v.days.length + 2}`}>${v.groups[g]} · ${xs.length}</div>
            ${xs.map((x) => html`<div class="cl-name"><b>${x.name}</b><span>chf. ${x.nr} · ${x.what}</span>${x.toil && html`<span class="cl-toil">${x.toil}</span>`}</div>
              <${Days} v=${v} x=${x} o=${o} />
              <div class="cl-hours">${x.hours.map((h) => html`<div title=${h.label}><b class=${h.over ? 't-warn' : ''}>${tr('%{h} h', { h: h.h })}</b>${h.of ? html` <i>${tr('of %{h}', { h: h.of })}</i>` : null}
                ${h.pct != null && html`<span class="cl-pct"><i class=${h.over ? 'over' : ''} style=${`width:${h.pct}%`}></i></span>`}</div>`)}</div>`)}`)}
          ${list.length === 0 && html`<div class="pro-dim" style=${`grid-column:1 / span ${v.days.length + 2};padding:10px`}>${tr('No drivers of this kind.')}</div>`}
        </div>
        <div class="cl-legend">${v.legend.map(([k, l]) => html`<span><i class=${'cl-cell ' + k}></i>${l}</span>`)}</div>
      </div>
    </div>
    ${o.pop && html`<${Pop} date=${o.date || day} />`}
    ${o.wishes && html`<${Wishes} />`}
    ${o.trades && html`<${Trades} />`}
    ${o.reports && html`<${Reports} />`}
  </div>`;
}

// --- the answers watched (round G) -----------------------------------------------------------------

/** How often the answers are asked for wherever RosterFlow Pro is open (ms). */
const WATCH_EVERY = 4000;

/** (Round G, NexiconDK 5 November 2026) The answers to the company's questions, watched wherever
 * RosterFlow Pro is open (`rf_answers`): a yes come meanwhile said at the bottom once - "Nina Bach
 * sagde ja til vagt 1004" - unless its duty's box is open; any answer come: the views asked
 * again (the day's mark). The yes answers there when it begins are not said. */
export function AnswerWatch() {
  useEffect(() => {
    let gone = false;
    let known = null;
    let shown = -1;
    const look = () =>
      rfCmd('rf_answers', {})
        .then((r) => {
          if (gone || !r) return;
          const yes = r.yes || [];
          if (known) {
            const o = st();
            for (const y of yes) {
              if (known.has(y.id)) continue;
              const open = o.pop && o.n === y.n && o.date === y.date;
              if (!open) say(y.text);
            }
          }
          known = new Set(yes.map((y) => y.id));
          if (shown >= 0 && r.shown !== shown) bump();
          shown = r.shown ?? 0;
        })
        .catch(() => {});
    look();
    const t = setInterval(look, WATCH_EVERY);
    return () => {
      gone = true;
      clearInterval(t);
    };
  }, []);
  return null;
}

// --- 4 Daglig drift ----------------------------------------------------------------------------------

export function DailyTab({ onCalendar }) {
  const o = st();
  const [v] = useView('rf_ops', { date: o.date || null }, [o.date]);
  if (!v) return html`<div class="rf-reading">${tr('Reading the timetable…')}</div>`;
  if (v.reading || v.off) return html`<div class="rf-reading">${v.reading}</div>`;
  const go = (date) => setO({ date, n: null, pop: false });
  return html`<div class="pro-page op-page">
    <div class="rf-toolbar pro-tools">
      <button type="button" class="rf-link" onClick=${onCalendar}><${Icon} name="chevron_left" size=${17} /><span>${tr('Back to the calendar')}</span></button>
      <i class="pro-sep"></i>
      <span class="tu-week">
        <button type="button" class="rf-ibtn" disabled=${!v.prev} title=${tr('The day before')} onClick=${() => go(v.prev)}><${Icon} name="chevron_left" size=${17} /></button>
        <${RfButton} label="Today" disabled=${v.today} onClick=${() => go(null)} />
        <button type="button" class="rf-ibtn" title=${tr('The day after')} onClick=${() => go(v.next)}><${Icon} name="chevron_right" size=${17} /></button>
      </span>
      <span class="grow"></span>
      <span class="tu-week"><b>${v.label}</b><span>${v.count?.text || ''}</span></span>
    </div>
    <div class="pro-main">
      <aside class="pro-side">
        ${v.frames.length > 0 && html`<div class="pro-caps">${tr('Frames this day')}</div>
          ${v.frames.map((f) => html`<div class="op-line"><span class="op-kind reserve">${tr('Reserve frame')}</span><span><b>${f.name}</b> ${f.time}</span><span class="pro-dim">${f.used ? f.what : tr('free')}</span></div>`)}`}
        ${v.free.length > 0 && html`<div class="pro-caps" style="margin-top:14px">${tr('Free all day')}</div>
          ${v.free.map((f) => html`<div class="op-line"><span class=${'op-kind ' + f.kind}>${f.chip}</span><span><b>${f.name}</b> chf. ${f.nr}</span></div>`)}`}
        <div class="pro-caps" style="margin-top:14px">${tr('Reserve frame')}</div>
        <div class="pro-dim">${tr('A reserve given a duty from 12:00 the day before is told at once.')}</div>
      </aside>
      <div class="pro-content op-daily"><${DayList} date=${v.date} /></div>
    </div>
    ${o.pop && html`<${Pop} date=${v.date} />`}
  </div>`;
}
