// The Staff page's Ticket inspection tab (NexiconDK, ticket inspection round S, 9 November 2026;
// `people::inspection::part`): today's teams, the checks of a day closed, the totals of the last 7
// and 30 days, and every line's share of fare dodgers with its trend and why.
import { html, setUi, goKey, Icon, Btn, Chips, Section, Meter, Tag, fill } from './kit.js';

// (the tab's own look, loaded with it)
const sheet = document.createElement('link');
sheet.rel = 'stylesheet';
sheet.href = 'omsihub/company-inspection.css';
document.head.append(sheet);

/** A line's share of dodgers over the last days (null: no passengers that day). */
function Trend({ values, tone, tip }) {
  const w = 120;
  const h = 26;
  const pts = values.map((v, k) => [k, v]).filter((p) => p[1] != null);
  if (pts.length < 2) return html`<span class="ti-trend none" title=${tip}></span>`;
  const top = Math.max(0.08, ...pts.map((p) => p[1]));
  const x = (k) => (values.length <= 1 ? 0 : (k / (values.length - 1)) * (w - 4) + 2);
  const y = (v) => h - 3 - (v / top) * (h - 6);
  const d = pts.map(([k, v], i) => `${i ? 'L' : 'M'}${x(k).toFixed(1)},${y(v).toFixed(1)}`).join(' ');
  const [lk, lv] = pts[pts.length - 1];
  return html`<svg class="ti-trend" width=${w} height=${h} viewBox=${`0 0 ${w} ${h}`}><title>${tip}</title>
    <path d=${d} fill="none" stroke=${fill(tone)} stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" />
    <circle cx=${x(lk)} cy=${y(lv)} r="2.6" fill=${fill(tone)} /></svg>`;
}

export function Inspection({ b }) {
  if (b.empty) {
    return html`<div class="co-locked" style="margin-top:4px"><${Icon} name="confirmation_number" size=${22} class="t-accent2" /><div>
      <p class="co-para" style="max-width:820px">${b.empty.text}</p>
      ${b.empty.go && html`<${Btn} kind="primary" icon=${b.empty.go.icon} label=${b.empty.go.label} style="height:40px;margin-top:16px" onClick=${() => goKey(b.empty.go.to)} />`}
      ${b.empty.hire && html`<${Btn} kind="primary" icon="badge" label=${b.empty.hire} style="height:40px;margin-top:16px" onClick=${() => setUi('people', { tab: 1, hire_role: b.empty.hire_role })} />`}
    </div></div>`;
  }
  return html`<div class="ti-page">
    <div class="ti-left co-scroll">
      <${Section} title=${b.today_title} style="flex:none">
        ${b.teams.length === 0 && html`<p class="co-note t-dim">${b.no_teams}</p>`}
        ${b.teams.map((t) => html`<div class="ti-team"><span class="ti-line">${t.line}</span><b class="cut">${t.who}</b>${t.alone && html`<${Tag} text=${b.alone} tone="warn" />`}<span class="t-soft">${t.time}</span></div>`)}
      </${Section}>
      <${Section} title=${b.totals_title} style="flex:none;margin-top:14px">
        <div class="ti-totals">
          <span></span>${b.totals.map((t) => html`<b>${t.label}</b>`)}
          ${b.total_cols.map((label, k) => {
            const key = ['checks', 'checked', 'caught', 'rate', 'written', 'paid'][k];
            return html`<span class="t-soft">${label}</span>${b.totals.map((t) => html`<span class=${key === 'paid' ? 't-ok' : ''}>${t[key]}</span>`)}`;
          })}
        </div>
      </${Section}>
      <${Section} title=${b.lines_title} style="flex:none;margin-top:14px">
        <p class="co-note t-dim" style="margin:-4px 0 10px">${b.lines_note}</p>
        ${b.lines.map(
          (l) => html`<div class="ti-lrow">
            <span class="ti-line">${l.number}</span>
            <div class="ti-lmid"><div class="ti-lbar"><b class=${'t-' + l.tone}>${l.share}</b><${Meter} frac=${l.frac} tone=${l.tone} h=${8} style="flex:1" /></div>
              <div class="ti-ltags"><span class="t-dim">${l.raw}  ·  ${l.checked}</span>${l.tags.map(([text, tone]) => html`<${Tag} text=${text} tone=${tone} />`)}</div></div>
            <${Trend} values=${l.trend} tone=${l.tone} tip=${b.trend_tip} />
          </div>`,
        )}
      </${Section}>
    </div>
    <div class="ti-right">
      <${Section} title=${b.checks_title} style="flex:1;min-height:0">
        ${b.days.length > 0 && html`<div style="flex:none;margin-bottom:10px"><${Chips} value=${b.day} options=${b.days.map((d, k) => [k, d])} onChange=${(k) => setUi('people', { day: k })} h=${30} /></div>`}
        ${b.day_sum && html`<p class="co-note t-soft" style="flex:none;margin-bottom:10px">${b.day_label}  ·  ${b.day_sum}</p>`}
        ${b.checks.length === 0
          ? html`<p class="co-note t-dim">${b.no_checks}</p>`
          : html`<div class="ti-checks co-scroll">
              <div class="ti-crow head">${b.cols.map((c) => html`<span>${c}</span>`)}</div>
              ${b.checks.map(
                (k) => html`<div class=${'ti-crow' + (k.caught > 0 ? ' caught' : '')}>
                  <span>${k.time}</span><span class="ti-line">${k.line}</span><span class="cut" title=${k.trip}>${k.trip}</span><span class="cut t-soft" title=${k.who}>${k.who}</span>
                  <span>${k.checked}</span><b class=${k.caught > 0 ? 't-warn' : 't-dim'}>${k.caught}</b><span class=${k.caught > 0 ? '' : 't-dim'}>${k.fines}</span>
                </div>`,
              )}
            </div>`}
      </${Section}>
    </div>
  </div>`;
}
