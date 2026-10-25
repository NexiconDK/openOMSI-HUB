// Bus Company Online (NexiconDK, phase 2 step 2; the Rust side is `omsihub/bco.rs`, the server's
// `company::server_api`): the third way to play at the bus company's start. The server's code
// once, then the player logged in with his Omsi-Hub account and/or this computer's Steam account,
// the server's companies to join with a click (the company's menu is "Play together"'s:
// `JoinPage`) and a company founded on a map of the server's from his own money there. Every text
// comes from Rust, translated.

import { html, useEffect, useState, setCo, cmd, Icon, Mark, Btn, Seg, Select, Dialog } from './kit.js';
import { run } from './multiplayer.js';
import { toast } from '../../store.js';

/** The server's code and the ways this computer logs in. */
function Connect({ v, again }) {
  const w = v.words;
  const [code, setCode] = useState(v.code || '');
  const [busy, setBusy] = useState(false);
  const go = async () => {
    setBusy(true);
    const r = await run('bco_connect', { code });
    setBusy(false);
    if (r) again(r);
  };
  // (the server's code kept from before: straight in, once)
  useEffect(() => {
    if (v.code && !v.problem) go();
  }, []);
  return html`<div class="co-choose tg-join bco">
    <div style="flex:none"><${Btn} icon="chevron_left" label=${w.back} style="width:180px;height:38px" onClick=${() => setCo({ chooser: 'mode' })} /></div>
    <div class="co-scroll" style="flex:1;margin-top:12px;display:flex;flex-direction:column;align-items:center">
      <span style="flex:0.3 1 0"></span>
      <div class="tg-joincard">
        <span class=${'tg-hero-icon big' + (busy ? ' pulse' : '')}><${Icon} name="public" size=${34} /></span>
        <div class="head"><h1>${w.title}</h1><p>${w.text}</p></div>
        <div class="tg-field"><input class="co-field" value=${code} placeholder=${w.code_field} onInput=${(e) => setCode(e.currentTarget.value)} onKeyDown=${(e) => e.key === 'Enter' && code.trim() && !busy && go()} />
          <${Btn} kind="primary" icon="link" label=${busy ? w.connecting : w.connect} disabled=${!code.trim() || busy} onClick=${go} /></div>
        ${v.problem && html`<p class="t-warn tg-why">${v.problem}</p>`}
        <ul class="tg-need">${(w.need || []).map((t) => html`<li>${t}</li>`)}</ul>
        ${v.ways?.steam && html`<p class="bco-way"><${Icon} name="sports_esports" size=${16} />${v.ways.steam}</p>`}
      </div>
      <span style="flex:0.7 1 0"></span>
    </div>
  </div>`;
}

/** A company of the server: its mark, map and mode, its owner, his role in it, Join. */
function CompanyRow({ c, w, onJoin }) {
  return html`<div class=${'bco-company' + (c.mine ? ' mine' : '')} data-company=${c.id}>
    <${Mark} mark=${{ colours: c.colours, short: c.short }} size=${48} />
    <div class="who">
      <b class="cut" title=${c.name}>${c.name}</b>
      <span class="dim cut">${c.map} · ${c.mode} · ${w.owner}: ${c.owner}</span>
      <span class="dim cut">${c.figures}</span>
    </div>
    ${c.role && html`<span class="bco-role">${c.mine ? w.mine : `${w.your_role}: ${c.role}`}</span>`}
    <span class=${'bco-state' + (c.runs ? ' on' : '')}><i></i>${c.state}</span>
    <${Btn} kind="primary" icon="groups" label=${w.join} disabled=${!c.runs} onClick=${onJoin} />
  </div>`;
}

/** Founding a company on the server: its name, map and game mode, and the price. */
function FoundDialog({ v, onClose, done }) {
  const w = v.words;
  const [name, setName] = useState('');
  const [map, setMap] = useState(0);
  const [mode, setMode] = useState('normal');
  const [busy, setBusy] = useState(false);
  const go = async () => {
    setBusy(true);
    const r = await run('bco_found', { name, map: v.maps[map]?.key || '', mode });
    setBusy(false);
    if (r && !r.error) {
      toast(r.message, 'ok');
      done(r.view);
    }
  };
  return html`<${Dialog} icon="add" title=${w.found_title} w=${560} onClose=${onClose}
    foot=${html`<span class="rest"></span><${Btn} label=${w.cancel} onClick=${onClose} /><${Btn} kind="primary" icon="add" label=${w.found_go} disabled=${!name.trim() || busy || !v.me.can_found || !v.maps.length} onClick=${go} />`}>
    <p class="co-para">${w.found_text}</p>
    <label class="bco-label">${w.name}</label>
    <input class="co-field" style="width:100%;height:40px" value=${name} onInput=${(e) => setName(e.currentTarget.value)} />
    <label class="bco-label">${w.map}</label>
    <${Select} value=${map} options=${v.maps.map((m) => m.label)} onChange=${setMap} style="width:100%" />
    <label class="bco-label">${w.mode}</label>
    <${Seg} value=${mode} options=${w.modes.map((m) => [m.key, m.label])} onChange=${setMode} />
    <p class=${'co-para ' + (v.me.can_found ? 'dim' : 't-warn')} style="margin-top:14px">${v.price}</p>
  </${Dialog}>`;
}

/** Logged in: the server, the player and his money, the companies. */
function Server({ v, again }) {
  const w = v.words;
  const [founding, setFounding] = useState(false);
  const [busy, setBusy] = useState(false);
  const refresh = async () => {
    setBusy(true);
    const r = await run('bco_refresh');
    setBusy(false);
    if (r) again(r);
  };
  const join = async (id) => {
    const r = await run('bco_join', { company: id });
    if (r?.joined) setCo({ chooser: 'join' });
  };
  const forget = async () => again(await run('bco_forget'));
  return html`<div class="tg-company bco">
    <div class="tg-head">
      <${Btn} icon="chevron_left" label=${w.back} style="width:140px;height:38px" onClick=${() => setCo({ chooser: 'mode' })} />
      <span class="tg-hero-icon"><${Icon} name="public" size=${26} /></span>
      <div class="who"><b class="cut">${v.server.name}</b><span class="cut">${w.server_time}: ${v.server.time} · ${w.players}: ${v.server.players}</span></div>
      <div class="who bco-me"><b class="cut">${v.me.name}</b><span class="cut">${w.money}: ${v.me.money} · ${v.me.by}</span></div>
      <${Btn} icon="refresh" label=${w.refresh} disabled=${busy} onClick=${refresh} />
      <${Btn} kind="primary" icon="add" label=${w.found} onClick=${() => setFounding(true)} />
      <${Btn} icon="logout" label=${w.forget} onClick=${forget} />
    </div>
    ${v.note && html`<p class="tg-banner t-warn"><${Icon} name="info" size=${16} />${v.note}</p>`}
    ${v.problem && html`<p class="tg-banner t-warn"><${Icon} name="warning" size=${16} />${v.problem}</p>`}
    <div class="co-scroll bco-list">
      <div class="co-caps">${w.companies_caps}</div>
      ${v.companies.length === 0 && html`<p class="dim">${w.none}</p>`}
      ${v.companies.map((c) => html`<${CompanyRow} c=${c} w=${w} onJoin=${() => join(c.id)} />`)}
    </div>
    ${founding && html`<${FoundDialog} v=${v} onClose=${() => setFounding(false)} done=${(nv) => (setFounding(false), nv && again(nv))} />`}
  </div>`;
}

/** Bus Company Online: the server's code, then the server. */
export function OnlinePage() {
  const [v, setV] = useState(null);
  useEffect(() => {
    cmd('bco_view')
      .then(setV)
      .catch((e) => toast(e.message, 'error'));
  }, []);
  // (the companies' state now and then, while the page is open)
  useEffect(() => {
    if (!v?.linked) return undefined;
    const t = setInterval(() => {
      cmd('bco_refresh')
        .then(setV)
        .catch(() => {});
    }, 15000);
    return () => clearInterval(t);
  }, [v?.linked]);
  if (!v) return null;
  if (v.off) return null;
  const again = (nv) => nv && !nv.error && setV(nv);
  if (!v.linked) return html`<${Connect} v=${v} again=${again} />`;
  return html`<${Server} v=${v} again=${again} />`;
}
