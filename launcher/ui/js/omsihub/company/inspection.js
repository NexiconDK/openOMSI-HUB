// The Staff page's Ticket inspection tab (NexiconDK, ticket inspection round S, 9 November 2026;
// `people::inspection::part`): today's teams, the checks of a day closed, the totals of the last 7
// and 30 days, and every line's share of fare dodgers with its trend and why.
// (round T, 10 November 2026) Its Plan part: the boss's inspection duties of the week and of single
// dates, storaktions at a stop, what lacks inspectors, and what was planned against what was done
// (`people::inspection::plan_part`); a duty's form over it.
import { html, useState, setUi, goKey, act, Icon, Btn, Seg, Chips, Section, Meter, Tag, Dialog, Select, fill } from './kit.js';

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

const hhmm = (m) => {
  const x = ((m % 1440) + 1440) % 1440;
  return `${String(Math.floor(x / 60)).padStart(2, '0')}:${String(x % 60).padStart(2, '0')}`;
};
/** The half hours of a day, for the form's times. */
const HALVES = Array.from({ length: 48 }, (_, k) => k * 30);

/** A duty in the week's grid or a date's list. */
function DutyCard({ d, onOpen }) {
  return html`<button type="button" class=${'ti-duty' + (d.action ? ' action' : '')} onClick=${onOpen} title=${d.via || d.what}>
    <span class="t">${d.time}</span>
    <b>${d.action ? html`<${Icon} name="groups" size=${14} /> ` : ''}${d.what}</b>
    ${d.via && html`<span class="t-soft">${d.via}</span>`}
    <span class="t-dim">${d.team}</span>
  </button>`;
}

/** A duty's form: its kind, days (or date), hours, lines or stop, and team. */
function DutyForm({ f, p, start, close }) {
  const [d, setD] = useState(start);
  const set = (patch) => setD({ ...d, ...patch });
  const action = d.kind === 'action';
  const range = action ? f.action_range : f.team_range;
  const toggle = (list, v) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
  const len = (((d.to - d.from) % 1440) + 1440) % 1440 || 1440;
  const stopK = p.stops.findIndex((s) => s.stop === d.stop);
  const save = async () => {
    const duty = { ...d, size: d.named.length ? d.named.length : d.size };
    const r = await act('insp_duty_save', { date: d.date || '', duty });
    if (r && !r.popup) close();
  };
  const remove = async () => {
    const r = await act('insp_duty_remove', { id: d.id });
    if (r && !r.popup) close();
  };
  const foot = html`${d.id ? html`<${Btn} kind="danger" icon="delete" label=${f.delete} onClick=${remove} />` : ''}<span class="rest"></span>
    <${Btn} label=${f.cancel} onClick=${close} /><${Btn} kind="primary" icon="check" label=${f.save} onClick=${save} />`;
  return html`<${Dialog} icon=${action ? 'groups' : 'confirmation_number'} title=${d.id ? f.edit : action ? f.new_action : f.new_patrol} w=${680} onClose=${close} foot=${foot}>
    <div class="ti-form">
      <label>${f.kind}</label><${Seg} value=${d.kind} options=${[['patrol', f.patrol], ['action', f.action]]} onChange=${(k) => set({ kind: k, size: Math.min(Math.max(d.size, k === 'action' ? f.action_range[0] : f.team_range[0]), k === 'action' ? f.action_range[1] : f.team_range[1]) })} style="height:36px" />
      ${d.date
        ? html`<label>${f.date}</label><b>${d.date_label}</b>`
        : html`<label>${f.days}</label><div class="ti-daychips">${f.heads.map((h, k) => html`<button type="button" class=${d.days.includes(k) ? 'on' : ''} onClick=${() => set({ days: toggle(d.days, k).sort() })}>${h}</button>`)}</div>`}
      <label title=${f.hours_tip}>${f.from} / ${f.to}</label>
      <div class="ti-hours">
        <${Select} value=${Math.max(0, HALVES.indexOf(((d.from % 1440) + 1440) % 1440))} options=${HALVES.map(hhmm)} onChange=${(k) => set({ from: HALVES[k], to: HALVES[k] + len })} style="width:120px" h=${36} />
        <span>–</span>
        <${Select} value=${Math.max(0, HALVES.indexOf(((d.to % 1440) + 1440) % 1440))} options=${HALVES.map(hhmm)} onChange=${(k) => set({ to: HALVES[k] <= d.from % 1440 ? HALVES[k] + 1440 : HALVES[k] })} style="width:120px" h=${36} />
        <span class="t-dim">${Math.floor(len / 60)}:${String(len % 60).padStart(2, '0')} t  ·  ${f.hours_tip}</span>
      </div>
      ${action
        ? html`<label>${f.stop}</label>
            ${p.stops.length
              ? html`<div><${Select} value=${stopK < 0 ? 0 : stopK + 1} options=${[f.stop_pick, ...p.stops.map((s) => `${s.stop}  (${s.lines})`)]} onChange=${(k) => set(k ? { stop: p.stops[k - 1].stop, through: p.stops[k - 1].through } : { stop: '', through: [] })} style="width:100%" h=${36} />
                  ${stopK >= 0 && html`<p class="co-note t-soft" style="margin-top:6px">${f.through}: ${p.stops[stopK].lines}</p>`}</div>`
              : html`<p class="co-note t-dim">${d.stop ? `${d.stop}  ·  ` : ''}${f.no_stops}</p>`}
            <span></span><p class="co-note t-dim">${f.action_tip}</p>`
        : html`<label title=${f.busiest_tip}>${f.lines}</label>
            <div class="ti-daychips">${p.lines.map((l) => html`<button type="button" class=${d.lines.includes(l.name) ? 'on' : ''} onClick=${() => set({ lines: toggle(d.lines, l.name) })}>${l.number}</button>`)}
              ${d.lines.length === 0 && html`<span class="t-dim" style="align-self:center">${f.busiest}</span>`}</div>`}
      <label>${f.size}</label>
      <div class="ti-hours">
        <${Btn} icon="remove" small disabled=${d.named.length > 0 || d.size <= range[0]} onClick=${() => set({ size: d.size - 1 })} />
        <b style="min-width:26px;text-align:center">${d.named.length || d.size}</b>
        <${Btn} icon="add" small disabled=${d.named.length > 0 || d.size >= range[1]} onClick=${() => set({ size: d.size + 1 })} />
        <span class="t-dim">${range[0]}–${range[1]}</span>
      </div>
      <label title=${f.named_tip}>${f.named}</label>
      <div class="ti-daychips">${p.inspectors.map((e) => html`<button type="button" class=${d.named.includes(e.id) ? 'on' : ''} disabled=${!d.named.includes(e.id) && d.named.length >= range[1]} onClick=${() => set({ named: toggle(d.named, e.id) })}>${e.name}</button>`)}</div>
      <span></span><p class="co-note t-dim">${f.named_tip}</p>
    </div>
  </${Dialog}>`;
}

/** The Plan part (`people::inspection::plan_part`). */
function Plan({ p }) {
  const [form, setForm] = useState(null);
  const [day, setDay] = useState('');
  const f = p.form;
  const blank = (kind, extra = {}) => ({ id: 0, kind, days: [], lines: [], from: kind === 'action' ? 15 * 60 : 8 * 60, to: kind === 'action' ? 18 * 60 : 16 * 60, size: kind === 'action' ? 4 : 2, named: [], stop: '', through: [], ...extra });
  const open = (d, extra = {}) => setForm({ ...blank(d.kind), ...d, ...extra });
  return html`<div class="ti-page">
    <div class="ti-left co-scroll">
      <${Section} title=${p.week_title} style="flex:none"
        tools=${html`<${Btn} small icon="add" label=${p.add_patrol} onClick=${() => setForm(blank('patrol'))} /><${Btn} small icon="groups" label=${p.add_action} onClick=${() => setForm(blank('action'))} />`}>
        ${p.auto && html`<div class="co-iconline t-ok" style="margin-bottom:8px"><${Icon} name="autorenew" size=${17} /><span><b>${p.auto_text}</b></span></div><p class="co-note t-dim" style="margin-bottom:10px">${p.auto_note}</p>`}
        <div class="ti-week">
          ${p.heads.map((h, k) => html`<div class="col"><div class="head">${h}</div>
            ${p.week[k].length === 0 ? html`<span class="t-dim auto">${p.auto_text}</span>` : p.week[k].map((d) => html`<${DutyCard} d=${d} onOpen=${() => open(d)} />`)}
            <button type="button" class="ti-add" title=${p.add_patrol} onClick=${() => setForm(blank('patrol', { days: [k] }))}>+</button>
          </div>`)}
        </div>
      </${Section}>
      <${Section} title=${p.dates_title} style="flex:none;margin-top:14px">
        <p class="co-note t-dim" style="margin:-4px 0 10px">${p.dates_note}</p>
        <div class="ti-hours" style="margin-bottom:10px">
          <input type="date" class="co-field" style="height:34px;width:170px" min=${p.today} value=${day} onInput=${(e) => setDay(e.currentTarget.value)} />
          <${Btn} small icon="event" label=${p.add_date} disabled=${!day} onClick=${() => act('insp_date_add', { date: day }).then(() => setDay(''))} />
        </div>
        ${p.dates.map((x) => html`<div class="ti-date">
          <div class="ti-hours"><b>${x.label}</b><span class="grow"></span>
            <${Btn} small icon="add" label=${p.add_patrol} onClick=${() => setForm(blank('patrol', { date: x.date, date_label: x.label }))} />
            <${Btn} small icon="groups" label=${p.add_action} onClick=${() => setForm(blank('action', { date: x.date, date_label: x.label }))} />
            <${Btn} small icon="restart_alt" label=${p.remove_date} onClick=${() => act('insp_date_remove', { date: x.date })} /></div>
          <div class="ti-daterow">${x.auto ? html`<span class="t-dim">${p.date_auto}</span>` : x.duties.map((d) => html`<${DutyCard} d=${d} onOpen=${() => open(d, { date: x.date, date_label: x.label })} />`)}</div>
        </div>`)}
      </${Section}>
    </div>
    <div class="ti-right co-scroll">
      <${Section} title=${p.warn_title} style="flex:none">
        ${p.warnings.length === 0
          ? html`<p class="co-note t-ok">${p.no_warnings}</p>`
          : p.warnings.map((w) => html`<div class=${'co-iconline t-' + w.tone}><${Icon} name="warning" size=${17} /><span>${w.text}</span></div>`)}
      </${Section}>
      <${Section} title=${p.done_title} style="flex:1;min-height:0;margin-top:14px">
        <p class="co-note t-dim" style="margin:-4px 0 8px">${p.done_note}</p>
        ${p.done.length === 0
          ? html`<p class="co-note t-dim">${p.no_done}</p>`
          : html`<div class="ti-done">
              <div class="ti-drow head">${p.done_cols.map((c) => html`<span>${c}</span>`)}</div>
              ${p.done.map((r) => html`<div class=${'ti-drow' + (r.auto ? ' auto' : '')}><span>${r.day}</span><b class="cut">${r.what}</b><span>${r.time}</span><span class="cut t-soft" title=${r.team}>${r.team}</span>
                <span class=${r.short ? 't-warn' : 't-ok'}>${r.staffed}</span><span>${r.trips}</span><b class=${r.caught > 0 ? 't-warn' : 't-dim'}>${r.caught}</b></div>`)}
            </div>`}
      </${Section}>
    </div>
    ${form && html`<${DutyForm} f=${f} p=${p} start=${form} close=${() => setForm(null)} key=${form.id + ':' + (form.date || '') + ':' + form.kind} />`}
  </div>`;
}

/** A trip checked. */
const CheckRow = ({ k, grouped }) => html`<div class=${'ti-crow' + (k.caught > 0 ? ' caught' : '')}>
  <span>${k.time}</span><span class="ti-line">${k.line}</span><span class="cut" title=${k.trip}>${k.trip}</span><span class="cut t-soft" title=${k.who}>${grouped ? '' : k.who}</span>
  <span>${k.checked}</span><b class=${k.caught > 0 ? 't-warn' : 't-dim'}>${k.caught}</b><span class=${k.caught > 0 ? '' : 't-dim'}>${k.fines}</span>
</div>`;

export function Inspection({ b }) {
  if (b.empty) {
    return html`<div class="co-locked" style="margin-top:4px"><${Icon} name="confirmation_number" size=${22} class="t-accent2" /><div>
      <p class="co-para" style="max-width:820px">${b.empty.text}</p>
      ${b.empty.go && html`<${Btn} kind="primary" icon=${b.empty.go.icon} label=${b.empty.go.label} style="height:40px;margin-top:16px" onClick=${() => goKey(b.empty.go.to)} />`}
      ${b.empty.hire && html`<${Btn} kind="primary" icon="badge" label=${b.empty.hire} style="height:40px;margin-top:16px" onClick=${() => setUi('people', { tab: 1, hire_role: b.empty.hire_role })} />`}
    </div></div>`;
  }
  const parts = html`<div style="flex:none;margin-bottom:12px"><${Seg} value=${b.part} options=${b.parts.map((t, k) => [k, t])} onChange=${(k) => setUi('people', { insp_part: k })} style="width:320px;height:36px" /></div>`;
  if (b.part === 1 && b.plan) return html`<div class="ti-wrap">${parts}<${Plan} p=${b.plan} /></div>`;
  return html`<div class="ti-wrap">${parts}<div class="ti-page">
    <div class="ti-left co-scroll">
      <${Section} title=${b.today_title} style="flex:none">
        ${b.teams.length === 0 && html`<p class="co-note t-dim">${b.no_teams}</p>`}
        ${b.teams.map((t) => html`<div class="ti-team">${t.stop ? html`<span class="ti-line action">${t.stop}</span>` : html`<span class="ti-line">${t.line}</span>`}<b class="cut">${t.who}</b>${t.planned && html`<${Tag} text=${b.planned_tag} tone="accent" />`}${t.auto && html`<${Tag} text=${b.auto_tag} tone="ok" />`}${t.alone && html`<${Tag} text=${b.alone} tone="warn" />`}<span class="t-soft">${t.time}</span></div>`)}
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
              ${b.checks.map((k) =>
                k.group
                  ? html`<div class="ti-group"><div class="ti-ghead"><${Icon} name="groups" size=${16} /><b>${k.group}</b><span class="t-soft cut">${k.who}</span><span class="grow"></span><span class="t-soft">${k.sum}</span></div>
                      ${k.rows.map((r) => html`<${CheckRow} k=${r} grouped=${true} />`)}</div>`
                  : html`<${CheckRow} k=${k} />`,
              )}
            </div>`}
      </${Section}>
    </div>
  </div></div>`;
}
