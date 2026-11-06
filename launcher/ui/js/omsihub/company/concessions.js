// The tenders (`company/concessions.rs`; Luc 2026-10-16, after the approved concept): the open
// tenders as cards - the contract they let, their tours, km a week and the authority's cap as
// chips, a countdown, the one picked in the accent -, the tender picked beside them as it runs
// with the company's clock - its timeline (open, bidding, award), the cap, the advice and your
// ask, how the asks are scored (70 % price, 30 % quality, your expected score as a bar), the
// rivals with their kind, quality and bid, placing the bid, buying the line outright, the clock
// stepped on for it - and below, your contracts with their rating and default points.

import { html, ui, setUi, Icon, Btn, Plate, Slider, Dialog, Seg } from './kit.js';
import { useMore, actMore, eur } from './more.js';
import { units, cents } from '../money.js';
import { Card, Empty, Chip, Dot, Track } from './pages-a.js';
import { VaryDialog, openVary } from './variations.js';

/** A small badge (`Ui::badge`): its words in capitals on a tint of its tone. */
export const Badge = ({ b }) => b && html`<${Chip} text=${b.text} tone=${b.tone} />`;

export function ConcessionsPage() {
  const st = ui('concessions', { amount: null, amount_for: null, bus: null, bus_for: null });
  const v = useMore('concessions', { amount: st.amount, amount_for: st.amount_for, bus: st.bus, bus_for: st.bus_for });
  if (!v?.tenders) return html`<div class="co-pg"></div>`;
  const t = v.tenders;
  return html`<div class="co-pg co-pg-tenders">
    <div class="co-pg-stack">
      ${t.note && html`<div class="co-pg-banner2"><${Icon} name="info" size=${18} /><span>${t.note}</span></div>`}
      <div class="co-pg-tendergrid">
        <${Card} title=${t.title} icon="campaign" tools=${t.count > 0 && html`<${Chip} text=${String(t.count)} tone="accent" />`} class="co-pg-tenderlist">
          ${t.empty
            ? html`<${Empty} icon="campaign" text=${t.empty} />`
            : html`<div class="co-pg-list">${t.rows.map((r) => html`<${Tender} r=${r} />`)}</div>`}
        </${Card}>
        <${Auction} a=${v.auction} st=${st} />
      </div>
      ${v.offers && html`<${Offers} o=${v.offers} />`}
      ${v.doubling && html`<${Doublings} o=${v.doubling} />`}
      ${v.changes && html`<${Changes} o=${v.changes} />`}
      ${v.subs && html`<${Subs} o=${v.subs} st=${st} />`}
      ${v.results && html`<${Results} r=${v.results} />`}
      <${Held} h=${v.held} />
    </div>
    <${VaryDialog} />
    ${st.terms != null && html`<${TermsDialog} r=${t.rows.find((r) => r.id === st.terms)} />`}
  </div>`;
}

/** (NexiconDK) The buses offered with a line won from another company (`concessions::offers`):
 * taken over all or none. */
function Offers({ o }) {
  return html`<${Card} title=${o.title} icon="directions_bus">
    <div class="co-pg-list">${o.rows.map(
      (r) => html`<div class="co-pg-tender" style="cursor:default">
        <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${24} /><b class="co-pg-itemtitle grow co-pg-cut">${r.title}</b></div>
        <p class="co-pg-note" style="margin:0">${r.say}</p>
        <div class="co-pg-chips">${r.buses.map(([b, v]) => html`<${Chip} text=${b + ' · ' + v} tone="dim" icon="directions_bus" />`)}</div>
        <div class="co-pg-line"><${Btn} kind="primary" icon="check_circle" label=${o.yes} onClick=${() => actMore('takeover_yes', { offer: r.id })} />
          <${Btn} icon="close" label=${o.no} onClick=${() => actMore('takeover_no', { offer: r.id })} /></div>
      </div>`,
    )}</div>
  </${Card}>`;
}

/** (NexiconDK, 3 November 2026) The authority's doubling orders (`concessions::doubling_orders`):
 * an extra bus behind a full departure, run or declined until 18:00 the day before. */
function Doublings({ o }) {
  return html`<${Card} title=${o.title} icon="directions_bus">
    <div class="co-pg-list">${o.rows.map(
      (r) => html`<div class="co-pg-tender" style="cursor:default">
        <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${24} /><b class="co-pg-itemtitle grow co-pg-cut">${r.title}</b></div>
        <p class="co-pg-note" style="margin:0">${r.say}</p>
        <div class="co-pg-chips"><${Chip} text=${r.state} tone=${r.ask ? 'accent' : 'dim'} icon="schedule" /></div>
        ${r.ask && html`<div class="co-pg-line"><${Btn} kind="primary" icon="check_circle" label=${o.yes} onClick=${() => actMore('doubling_yes', { order: r.id })} />
          <${Btn} icon="close" label=${o.no} onClick=${() => actMore('doubling_no', { order: r.id })} /></div>`}
      </div>`,
    )}</div>
  </${Card}>`;
}

/** (NexiconDK, 3 November 2026) The authority's changes of the company's contracts
 * (`concessions::changes`): a line added or taken out, the map's new timetable - declined while
 * the company may. */
function Changes({ o }) {
  return html`<${Card} title=${o.title} icon="edit_calendar">
    <div class="co-pg-list">${o.rows.map(
      (r) => html`<div class="co-pg-tender" style="cursor:default">
        <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${24} /><b class="co-pg-itemtitle grow co-pg-cut">${r.title}</b></div>
        <p class="co-pg-note" style="margin:0">${r.say}</p>
        <div class="co-pg-chips"><${Chip} text=${r.state} tone=${r.ask ? 'accent' : 'dim'} icon="schedule" /></div>
        ${r.ask && html`<div class="co-pg-line"><${Btn} icon="close" label=${o.decline} onClick=${() => actMore('change_decline', { change: r.id })} /></div>`}
      </div>`,
    )}</div>
  </${Card}>`;
}

/** (NexiconDK, 3 November 2026) The company's subcontracts (`concessions::subs`): proposed to it,
 * given, run - and a proposal of its own lines to another company of the map. */
function Subs({ o, st }) {
  const pick = (k, d) => (st['sub_' + k] ?? d);
  const set = (k) => (e) => setUi('concessions', { ['sub_' + k]: e.currentTarget.type === 'select-multiple' ? [...e.currentTarget.selectedOptions].map((x) => x.value) : e.currentTarget.value });
  const lines = pick('lines', []);
  return html`<${Card} title=${o.title} icon="handshake">
    <p class="co-pg-note" style="margin:0 0 8px">${o.hint}</p>
    ${o.rows.length > 0 && html`<div class="co-pg-list">${o.rows.map(
      (r) => html`<div class="co-pg-tender" style="cursor:default">
        <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${24} /><b class="co-pg-itemtitle grow co-pg-cut">${r.title}</b></div>
        <p class="co-pg-note" style="margin:0">${r.say}</p>
        <div class="co-pg-chips"><${Chip} text=${r.state} tone=${r.answer ? 'accent' : 'dim'} icon="handshake" /></div>
        <div class="co-pg-line">
          ${r.answer && html`<${Btn} kind="primary" icon="check" label=${o.accept} onClick=${() => actMore('sub_answer', { sub_id: r.id, yes: true })} /><${Btn} icon="close" label=${o.decline} onClick=${() => actMore('sub_answer', { sub_id: r.id, yes: false })} />`}
          ${r.withdraw && html`<${Btn} icon="close" label=${o.withdraw} onClick=${() => actMore('sub_answer', { sub_id: r.id, yes: false })} />`}
          ${r.end && html`<${Btn} kind="ghost" icon="event_busy" label=${o.end_label} onClick=${() => actMore('sub_end', { sub_id: r.id })} />`}
        </div>
      </div>`,
    )}</div>`}
    ${o.held.length > 0 && o.others.length > 0 && html`<div class="co-pg-line" style="flex-wrap:wrap;gap:8px;margin-top:8px">
      <label class="co-pg-small">${o.lines_label}<select multiple size=${Math.min(4, o.held.length)} onChange=${set('lines')}>${o.held.map((h) => html`<option value=${h.line} selected=${lines.includes(h.line)}>${h.number} (${h.until})</option>`)}</select></label>
      <label class="co-pg-small">${o.who_label}<select onChange=${set('who')}>${o.others.map((x) => html`<option value=${x.id} selected=${pick('who', o.others[0].id) === x.id}>${x.name}</option>`)}</select></label>
      <label class="co-pg-small">${o.price_label}<input type="number" min="0" step="1" style="width:90px" value=${pick('price', '')} onInput=${set('price')} /></label>
      <label class="co-pg-small">${o.from_label}<input type="date" min=${o.tomorrow} value=${pick('from', o.tomorrow)} onInput=${set('from')} /></label>
      <${Btn} kind="primary" icon="send" label=${o.propose} onClick=${() => actMore('sub_propose', { sub: pick('who', o.others[0].id), lines, hour: Math.round(cents(pick('price', 0))), from: pick('from', o.tomorrow) })} />
    </div>`}
  </${Card}>`;
}

/** (NexiconDK) What a tender's contract asks (`concessions::requirements`), from its card's
 * "Contract requirements". */
function TermsDialog({ r }) {
  const close = () => setUi('concessions', { terms: null });
  if (!r?.terms) return null;
  const d = r.terms;
  return html`<${Dialog} icon="description" title=${d.title} w=${680} onClose=${close} foot=${html`<span class="rest"></span><${Btn} kind="primary" label=${d.close} onClick=${close} />`}>
    <div class="co-terms">
      ${d.groups.map((g) => html`<div class="co-caps">${g.caps}</div>
        <div class="co-terms-rows">${g.rows.map(([k, v]) => html`<b>${k}</b><span>${v}</span>`)}</div>`)}
    </div>
  </${Dialog}>`;
}

/** (NexiconDK, 6 November 2026) The other lines a line's tours run - "kører også NE2 🌙, NE10 🌙"
 * (a night line with a moon, its word on hover). */
function Also({ lines, night }) {
  return lines.map((x, k) => html`${k > 0 ? ', ' : ''}<b class="co-pg-alsoline">${x.number}${x.night && html` <span class="co-pg-night" title=${night}>🌙</span>`}</b>`);
}

/** A tender's lines with the others their tours run (`concessions::runs_json`): one row a line of
 * a package. */
function Runs({ r }) {
  if (!r) return null;
  return html`<div class="co-pg-runs" title=${r.label + ': ' + r.all}>${r.rows
    .filter((x) => x.also.length > 0 || r.rows.length > 1)
    .map((x) => html`<span class="co-pg-run"><b>${x.number}</b>${x.also.length > 0 && html` · ${x.text} <${Also} lines=${x.also} night=${r.night} />`}</span>`)}</div>`;
}

/** A tender's card: its plate and name, its chips, its countdown, its state and your bid. */
function Tender({ r }) {
  return html`<button type="button" class=${'cm-tender co-pg-tender' + (r.on ? ' on' : '') + (r.dim ? ' past' : '')} onClick=${() => actMore('tender_pick', { id: r.id })}>
    <div class="co-pg-line">
      <${Plate} p=${{ number: r.number }} h=${24} />
      <b class="co-pg-itemtitle grow co-pg-cut" title=${r.caption}>${r.caption}</b>
      ${r.badge && html`<${Badge} b=${r.badge} />`}
    </div>
    <div class="co-pg-chips">${(r.chips || []).map((c) => html`<${Chip} text=${c.text} tone=${c.tone} icon=${c.icon} />`)}</div>
    <${Runs} r=${r.runs} />
    <div class="co-pg-line">
      <span class=${'co-pg-cut grow co-pg-small t-' + r.tone} title=${r.state}>${r.state}</span>
      ${r.countdown && html`<span class=${'co-pg-countdown t-' + r.countdown[1]}><${Icon} name="timer" size=${14} />${r.countdown[0]}</span>`}
    </div>
    <span class=${'co-pg-cut co-pg-small t-' + r.bid_tone} title=${r.bid}>${r.bid}</span>
    ${r.terms && html`<span role="button" tabindex="0" class="co-pg-terms" onClick=${(e) => (e.stopPropagation(), setUi('concessions', { terms: r.id }))}
      onKeyDown=${(e) => e.key === 'Enter' && (e.stopPropagation(), setUi('concessions', { terms: r.id }))}><${Icon} name="description" size=${15} />${r.terms_label}</span>`}
  </button>`;
}

/** The tender picked (`concessions::auction`). */
function Auction({ a, st }) {
  if (a.none) return html`<${Card} title=${a.title} icon="sports_score"><${Empty} icon="touch_app" text=${a.none} /></${Card}>`;
  const act = a.actions;
  const running = act.kind === 'running';
  const amount = running ? (st.amount_for === act.id && st.amount != null ? Math.min(act.hi, Math.max(act.lo, st.amount)) : act.amount) : null;
  const setAmount = (x) => setUi('concessions', { amount: Math.min(act.hi, Math.max(act.lo, x)), amount_for: act.id });
  // (NexiconDK, 3 November 2026: a bid in two parts - a contract bus a day besides the hour)
  const bus = running && act.bus ? act.bus.value : null;
  const setBus = (x) => setUi('concessions', { bus: Math.min(act.bus.max, Math.max(0, x)), bus_for: act.id });
  const shown = running ? a.figures.slice(0, a.running_tiles || 2) : a.figures;
  return html`<${Card} class="co-pg-auction">
    <div class="co-pg-line co-pg-auctionhead">
      <${Plate} p=${{ number: a.number }} h=${28} />
      <div class="grow" style="min-width:0"><b class="co-pg-auctiontitle co-pg-cut" title=${a.caption}>${a.caption}</b><span class=${'co-pg-small co-pg-cut t-' + a.tone}>${a.state}</span></div>
    </div>
    ${a.terms && html`<p class="co-pg-note">${a.terms}</p>`}
    ${a.sealed && html`<div class="co-pg-banner2"><${Icon} name="lock" size=${18} /><span>${a.sealed}</span></div>`}
    ${a.drive && html`<div class="co-pg-line" title=${a.drive.say}><span class="co-pg-caps">${a.drive.label}</span>
      <${Seg} value=${a.drive.value} options=${a.drive.options} onChange=${(k) => actMore('tender_drive', { id: a.id, drive: k })} style="width:300px;height:34px" /></div>
      <p class="co-pg-note" style="margin:0">${a.drive.say}</p>`}
    ${a.mine && html`<p class=${'co-pg-small t-' + a.mine_tone} style="font-weight:700;margin:0">${a.mine}</p>`}
    <${Timeline} steps=${a.timeline} frac=${a.frac} times=${a.times} />
    ${a.work && html`<div class="co-pg-tiles">
      ${a.work.map((f) => html`<div class="co-pg-tile"><span>${f.label}</span><b class="t-text">${f.value}</b>${f.sub && html`<span class="co-pg-small t-faint" style="text-transform:none;letter-spacing:0">${f.sub}</span>`}</div>`)}
    </div>`}
    <div class="co-pg-tiles">
      ${shown.map((f) => html`<div class=${'co-pg-tile' + (f.tone === 'ok' ? ' good' : '')} title=${f.tip || ''}><span>${f.label}</span><b class=${'t-' + f.tone}>${f.value}</b></div>`)}
      ${running &&
      html`<div class="co-pg-tile bid"><span>${act.label}</span>
        <input class="co-pg-bidin" type="number" step=${act.per_hour ? 1 : 100} value=${act.per_hour ? Math.round(units(amount * 100) * 100) / 100 : Math.round(units(amount * 100))} onChange=${(e) => setAmount(cents(e.currentTarget.value) / 100)} />
        <${Slider} value=${amount} min=${act.lo} max=${act.hi} step=${act.per_hour ? Math.max(0.01, cents(1) / 100) : 100} format=${(x) => eur(Math.round(x * 100))} onChange=${setAmount} /></div>`}
      ${running && act.bus &&
      html`<div class="co-pg-tile bid"><span>${act.bus.label}</span>
        <input class="co-pg-bidin" type="number" step="1" value=${Math.round(units(bus * 100))} onChange=${(e) => setBus(cents(e.currentTarget.value) / 100)} />
        <${Slider} value=${bus} min=${0} max=${act.bus.max} step=${Math.max(0.01, cents(1) / 100)} format=${(x) => eur(Math.round(x * 100))} onChange=${setBus} /></div>`}
    </div>
    ${running && html`<p class=${'co-pg-small t-' + act.chance_tone} style="margin:0;font-weight:700">${act.chance}</p>`}
    ${running && act.total && html`<p class="co-pg-small" style="margin:0;font-weight:700">${act.total}</p>`}
    ${running && act.recommend && html`<button type="button" class="co-pg-linkbtn" onClick=${() => setAmount(act.recommend_amount)}><${Icon} name="check_circle" size=${15} />${act.recommend}</button>`}
    ${a.score && html`<${Score} s=${a.score} />`}
    ${a.rivals.length > 0 &&
    html`<div><div class="co-pg-caps" style="margin-bottom:6px">${a.rivals_title}</div>
      <table class="co-pg-table co-pg-rivals">
        <colgroup><col /><col style="width:20%" /><col style="width:24%" /><col style="width:26%" /></colgroup>
        <thead><tr>${a.rivals_heads.map((h, k) => html`<th class=${k === 3 ? 'num' : ''}>${h}</th>`)}</tr></thead>
        <tbody>${a.rivals.map(
          (r) => html`<tr class=${r.leads ? 'leads' : ''} title=${r.points || ''}><td><b>${r.name}</b></td><td class="co-pg-muted">${r.kind}</td>
            <td><span class="co-pg-line"><${Track} frac=${r.quality} tone=${r.quality >= 0.65 ? 'ok' : r.quality >= 0.4 ? 'warn' : 'danger'} h=${6} /><span class="co-pg-small">${r.quality_text}</span></span></td>
            <td class=${'num t-' + r.tone}>${r.bid}</td></tr>`,
        )}</tbody>
      </table></div>`}
    <div class="co-pg-actions">
      ${running &&
      html`<${Btn} kind="primary" icon="payments" label=${act.bid} onClick=${() => actMore('tender_bid', { id: act.id, amount: Math.round(amount * 100), per_hour: !!act.per_hour, bus_day: bus != null ? Math.round(bus * 100) : 0 })} />
        ${act.buy && html`<${Btn} icon="check_circle" label=${act.buy} title=${act.buy_tip} onClick=${() => actMore('tender_buy', { id: act.id })} />`}
        ${act.plus15 && html`<${Btn} kind="ghost" icon="timer" label=${act.plus15} onClick=${() => actMore('tender_step', { id: act.id, to: '15' })} />`}
        ${act.to_close && html`<${Btn} kind="ghost" icon="sports_score" label=${act.to_close} onClick=${() => actMore('tender_step', { id: act.id, to: 'close' })} />`}`}
      ${act.kind === 'wait' && act.label && html`<${Btn} kind="primary" icon="schedule" label=${act.label} onClick=${() => actMore('tender_step', { id: act.id, to: 'open' })} />`}
      ${act.kind === 'said' && act.text && html`<p class=${'co-pg-small t-' + act.tone} style="margin:0;font-weight:700">${act.text}</p>`}
    </div>
    ${a.joint &&
    html`<div class="co-pg-score">
      <div class="co-pg-line"><${Icon} name="handshake" size=${16} class="t-accent" /><b class="grow">${a.joint.title}</b></div>
      <p class="co-pg-note" style="margin:0">${a.joint.hint}</p>
      <div class="co-pg-line" style="flex-wrap:wrap;gap:8px">
        <select onChange=${(e) => setUi('concessions', { joint_who: e.currentTarget.value })}>${a.joint.partners.map((x) => html`<option value=${x.id} selected=${(st.joint_who ?? a.joint.partners[0].id) === x.id}>${x.name}</option>`)}</select>
        <select multiple size=${Math.min(4, a.joint.lines.length)} onChange=${(e) => setUi('concessions', { joint_lines: [...e.currentTarget.selectedOptions].map((x) => x.value) })}>${a.joint.lines.map((l) => html`<option value=${l.line} selected=${(st.joint_lines || []).includes(l.line)}>${l.number}</option>`)}</select>
        <label class="co-pg-small">${a.joint.price_label}<input type="number" min="0" step="1" style="width:90px" value=${st.joint_price ?? ''} onInput=${(e) => setUi('concessions', { joint_price: e.currentTarget.value })} /></label>
        <${Btn} icon="send" label=${a.joint.button} onClick=${() => actMore('joint_propose', { id: a.id, sub: st.joint_who ?? a.joint.partners[0].id, lines: st.joint_lines || [], hour: Math.round(cents(st.joint_price || 0)) })} />
      </div>
    </div>`}
    ${a.complain &&
    html`<div class="co-pg-score">
      <div class="co-pg-line"><${Icon} name="gavel" size=${16} class="t-accent" /><b class="grow">${a.complain.title}</b></div>
      <p class="co-pg-note" style="margin:0">${a.complain.hint}</p>
      <textarea class="co-pg-bidin" style="width:100%;min-height:64px;box-sizing:border-box" placeholder=${a.complain.placeholder}
        value=${st.complaint_for === a.id ? st.complaint || '' : ''} onInput=${(e) => setUi('concessions', { complaint: e.currentTarget.value, complaint_for: a.id })}></textarea>
      <div class="co-pg-actions"><${Btn} icon="gavel" label=${a.complain.button} onClick=${() => actMore('tender_complain', { id: a.id, reason: st.complaint_for === a.id ? st.complaint || '' : '' })} /></div>
    </div>`}
    <details class="co-pg-log"><summary>${a.log_title} (${a.log.length})</summary>
      ${a.log.length
        ? a.log.map((l) => html`<div class="co-pg-line"><span class="co-pg-muted" style="width:48px">${l.at}</span><span class=${'grow co-pg-cut' + (l.me ? ' t-accent' : '')}>${l.who}</span><b>${l.amount}</b></div>`)
        : html`<p class="co-pg-note">${a.no_log}</p>`}
    </details>
  </${Card}>`;
}

/** (NexiconDK, 31 October 2026) The map's tenders as their sealed bids were opened at the close
 * (`concessions::results`): who won at what, and every bid with its points, a rejected one marked. */
function Results({ r }) {
  return html`<${Card} title=${r.title} sub=${r.sub} icon="gavel">
    ${r.rows.length === 0
      ? html`<${Empty} small icon="gavel" text=${r.empty} />`
      : html`<div class="co-pg-list">${r.rows.map(
          (x) => html`<details class="co-pg-tender co-pg-result">
            <summary class="co-pg-line"><${Plate} p=${{ number: x.number }} h=${22} /><b class="co-pg-itemtitle grow co-pg-cut" title=${x.caption}>${x.caption}</b>
              <span class=${'co-pg-small t-' + x.tone} style="font-weight:700">${x.said}</span><span class="co-pg-small co-pg-muted">${x.closed}</span></summary>
            ${x.failed && html`<p class="co-pg-note" style="margin:0">${x.failed}</p>`}
            <table class="co-pg-table co-pg-rivals"><tbody>${x.bids.map(
              (b) => html`<tr class=${b.won ? 'leads' : ''} title=${b.points}><td><b class=${b.me ? 't-accent' : ''}>${b.who}</b></td>
                <td class="co-pg-muted co-pg-small">${b.points}</td><td class=${'num ' + (b.rejected ? 't-danger' : b.won ? 't-ok' : '')}>${b.bid}</td></tr>`,
            )}</tbody></table>
          </details>`,
        )}</div>`}
  </${Card}>`;
}

/** Open, bidding, award: where the tender stands, the bidding's progress under it. */
function Timeline({ steps, frac, times }) {
  return html`<div class="co-pg-timeline">
    <div class="co-pg-steps">${steps.map(
      (s, k) => html`<div class=${'co-pg-step ' + s.state}><span class="co-pg-stepdot">${s.state === 'done' ? html`<${Icon} name="check" size=${14} />` : k + 1}</span><b>${s.label}</b><span class="co-pg-small co-pg-muted co-pg-cut">${s.sub}</span></div>`,
    )}</div>
    <div class="co-pg-line"><${Track} frac=${frac} h=${6} /><span class="co-pg-small co-pg-muted" style="flex:none">${times}</span></div>
  </div>`;
}

/** How the asks are scored, and the score your ask would get: 70 price points, 30 quality. */
function Score({ s }) {
  return html`<div class="co-pg-score">
    <div class="co-pg-line"><${Icon} name="info" size=${16} class="t-accent" /><b class="grow">${s.title}</b><span class="co-pg-chip t-accent" style="background:rgba(var(--hub-accent-rgb),0.14)">${s.head}</span></div>
    <div class="co-pg-stacked" title=${s.total}>
      <i class="price" style=${`width:${s.price}%`}></i><i class="quality" style=${`width:${s.quality}%`}></i>
    </div>
    <div class="co-pg-line co-pg-small"><span class="co-pg-key price"></span><span>${s.price_label}</span><span class="co-pg-key quality"></span><span class="grow">${s.quality_label}</span><b>${s.total}</b></div>
    <ul class="co-pg-points">${s.lines.map((l) => html`<li>${l}</li>`)}</ul>
  </div>`;
}

/** The company's contracts: its concessions and own lines, each with its rating's dot, its
 * payment and its default points (`concessions::held`), and a change to request of a tendered
 * one (`variations.js`). */
function Held({ h }) {
  const rows = [...h.rows.map((r) => ({ ...r, own: false })), ...h.own.map((r) => ({ ...r, own: true }))];
  return html`<${Card} title=${h.title} sub=${h.sub} icon="description">
    ${h.empty
      ? html`<${Empty} small icon="description" text=${h.empty} />`
      : html`<div class="co-pg-heldgrid">${rows.map((r) => {
          const badge = r.rag || (r.direct && { text: r.direct, tone: 'dim' });
          return html`<div class="co-pg-item">
            <div class="co-pg-line"><${Plate} p=${{ number: r.number }} h=${22} />
              <b class="co-pg-itemtitle grow co-pg-cut">${r.caption || r.number}</b>
              ${r.rag ? html`<${Dot} tone=${r.rag.tone} title=${r.rag.text} /><span class=${'co-pg-small t-' + r.rag.tone} style="font-weight:800">${r.rag.text}</span>` : badge && html`<${Badge} b=${badge} />`}
            </div>
            ${r.also && html`<span class="co-pg-itemsub">${r.also.text} <${Also} lines=${r.also.lines} night=${r.also.night} /></span>`}
            ${!r.own && html`<span class=${'co-pg-itemsub t-' + r.tone}>${r.until}</span>`}
            <span class="co-pg-itemsub">${r.own ? r.text : r.price}</span>
            ${r.points && html`<span class="co-pg-itemsub"><${Icon} name="flag" size=${13} /> ${r.points}</span>`}
            ${r.unpaid && html`<span class="co-pg-itemsub">${r.unpaid}</span>`}
            ${r.change && html`<div><${Btn} icon="edit_road" label=${r.change} style="height:30px;padding:0 10px" onClick=${() => openVary(r.line)} /></div>`}
          </div>`;
        })}</div>`}
  </${Card}>`;
}
