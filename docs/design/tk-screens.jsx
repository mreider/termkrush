// tk-screens.jsx — the five TermKrush interface states.
// Each returns a full 1280×800 app frame; central content swaps per mode.
const { TopBar, LibraryPanel, Pad, ScratchPanel, Wave, Slider, padData } = window;

// ── 1 · MAIN VIEW ───────────────────────────────────────────────────────────
function ScreenMain() {
  return (
    <div className="tk-app">
      <TopBar bpm={92} gain={0.82} />
      <div className="tk-body">
        <LibraryPanel />
        <div className="tk-central">
          <div className="tk-phead"><span>Pads</span><span style={{ letterSpacing: 0, textTransform: 'none', color: 'var(--dimmer)' }}>8 cells · first loop sets tempo</span></div>
          <div className="tk-pads">
            {padData.map((p) => <Pad key={p.n} p={p} />)}
          </div>
        </div>
      </div>
      <ScratchPanel armed="scratch_ahh.wav" playheadPct={0} rotate={0} />
    </div>
  );
}

// ── 2 · CLIP EDITOR ─────────────────────────────────────────────────────────
function ScreenClip() {
  const region = [0.22, 0.68];
  const ticks = [0, 1, 2, 3, 4, 5, 6];
  return (
    <div className="tk-app">
      <TopBar bpm={92} gain={0.82} />
      <div className="tk-body">
        <LibraryPanel />
        <div className="tk-central">
          <div className="tk-editor">
            <div className="tk-ed-head">
              <div className="tk-ed-title">
                <span className="tag">edit</span>
                <span className="fn">amen_break.wav</span>
                <span style={{ fontSize: 11, color: 'var(--dim)' }}>· trim is non-destructive</span>
              </div>
              <button className="tk-btn">done ✓</button>
            </div>

            <div className="tk-ed-wavebox">
              <div className="tk-ruler">
                {ticks.map((t) => (
                  <span key={t} style={{ left: (t / 6 * 100) + '%' }}>{t}s</span>
                ))}
              </div>
              <div className="tk-ed-wave">
                <Wave seed={11} bars={170} region={region} tone="amber" />
              </div>
              <div className="tk-handle in" style={{ left: (region[0] * 100) + '%' }}>
                <div className="grab">◀</div>
              </div>
              <div className="tk-handle out" style={{ left: (region[1] * 100) + '%' }}>
                <div className="grab">▶</div>
              </div>
            </div>

            <div className="tk-ed-foot">
              <button className="tk-btn"><span className="play">▶</span> play selection</button>
              <button className="tk-btn amber">export trimmed → library</button>
              <div className="rd tk-tnum" style={{ marginLeft: 'auto' }}>
                <span>in <b>1.34s</b></span>
                <span>out <b>4.10s</b></span>
                <span>len <b>2.76s</b></span>
              </div>
            </div>
          </div>
        </div>
      </div>
      <ScratchPanel armed="scratch_ahh.wav" playheadPct={0} rotate={0} />
    </div>
  );
}

// ── 3 · SCRATCH — ARMED / FOCUSED ───────────────────────────────────────────
function ScreenScratch() {
  return (
    <div className="tk-app">
      <TopBar bpm={92} gain={0.82} />
      <div className="tk-body">
        <LibraryPanel />
        <div className="tk-central">
          <div className="tk-phead"><span>Pads</span><span style={{ letterSpacing: 0, textTransform: 'none', color: 'var(--dimmer)' }}>scratching — pads keep playing</span></div>
          <div className="tk-pads tk-dim" style={{ flex: 'none', gridTemplateRows: '1fr' }}>
            {padData.slice(0, 4).map((p) => <Pad key={p.n} p={p} />)}
          </div>

          {/* big focused scratch deck */}
          <div style={{ flex: 1, display: 'flex', minHeight: 0, borderTop: '1px solid var(--line)' }}>
            <div style={{ width: 300, flex: 'none', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRight: '1px solid var(--line)', position: 'relative' }}>
              {/* motion arc */}
              <svg width="240" height="240" style={{ position: 'absolute' }} viewBox="0 0 240 240" fill="none">
                <path d="M120 28 A92 92 0 0 1 205 86" stroke="var(--amber)" strokeWidth="2" strokeLinecap="round" opacity="0.5" />
                <path d="M203 80 l6 10 -12 1 z" fill="var(--amber)" opacity="0.6" />
              </svg>
              <div className="tk-platter" style={{ width: 196, height: 196, transform: 'rotate(26deg)' }}>
                <span className="marker" style={{ height: 36 }} />
              </div>
            </div>
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, padding: 18, gap: 14 }}>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <span style={{ width: 9, height: 9, borderRadius: '50%', background: 'var(--amber)', boxShadow: '0 0 9px var(--amber)' }} />
                  <span style={{ fontSize: 15 }}>scratch_ahh.wav</span>
                  <span style={{ fontSize: 10, letterSpacing: 2, color: 'var(--amber)', border: '1px solid var(--amber-ln)', padding: '2px 7px', borderRadius: 3, textTransform: 'uppercase' }}>armed</span>
                </div>
                <div style={{ display: 'flex', gap: 22, alignItems: 'baseline' }}>
                  <div style={{ textAlign: 'right' }}>
                    <div style={{ fontSize: 9, letterSpacing: 1, color: 'var(--dimmer)', textTransform: 'uppercase' }}>jog velocity</div>
                    <div className="tk-tnum" style={{ fontSize: 17, color: 'var(--amber)', fontWeight: 700 }}>+1.8×</div>
                  </div>
                  <div style={{ textAlign: 'right' }}>
                    <div style={{ fontSize: 9, letterSpacing: 1, color: 'var(--dimmer)', textTransform: 'uppercase' }}>direction</div>
                    <div style={{ fontSize: 17, color: 'var(--green)', fontWeight: 700 }}>WIKI ▶</div>
                  </div>
                </div>
              </div>

              <div className="tk-wavebox" style={{ flex: 1 }}>
                <div className="tk-playhead" style={{ left: '44%' }} />
                {/* faint ghost of where the head came from */}
                <div style={{ position: 'absolute', top: 0, bottom: 0, left: '31%', width: 1, background: 'var(--amber-ln)', opacity: 0.7 }} />
                <div style={{ position: 'absolute', inset: '10px 0', padding: '0 2px' }}>
                  <Wave seed={31} bars={150} tone="plain" />
                </div>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 10.5, color: 'var(--dim)' }}>
                <span>drag platter ↔ — drag speed sets jog velocity · held-still = silent</span>
                <span style={{ display: 'flex', gap: 12 }}>
                  <span>hold <span style={{ color: 'var(--ink)', border: '1px solid var(--line-2)', borderRadius: 3, padding: '1px 6px', background: 'var(--panel-2)' }}>←</span> whip</span>
                  <span><span style={{ color: 'var(--ink)', border: '1px solid var(--line-2)', borderRadius: 3, padding: '1px 6px', background: 'var(--panel-2)' }}>→</span> wiki</span>
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── 4 · TIMELINE (backlog proposal) ─────────────────────────────────────────
function ScreenTimeline() {
  const BARS = 8;
  const pc = (bar) => (bar / BARS * 100) + '%';
  const wb = (a, b) => ({ left: pc(a), width: ((b - a) / BARS * 100) + '%' });
  const lanes = [
    { nm: 'drums', ct: '2 clips', blocks: [{ a: 0, b: 4, nm: 'amen_break', tone: 'amber', seed: 11 }, { a: 4, b: 8, nm: 'amen_break', tone: 'amber', seed: 11 }] },
    { nm: 'keys', ct: '2 clips', blocks: [{ a: 2, b: 3, nm: 'rhodes', tone: 'green', seed: 23 }, { a: 6, b: 7, nm: 'rhodes', tone: 'green', seed: 23 }] },
    { nm: 'texture', ct: '1 clip', blocks: [{ a: 0, b: 8, nm: 'vinyl_crackle', tone: 'green', seed: 5 }] },
    { nm: 'scratch', ct: '1 clip', blocks: [{ a: 3, b: 3.6, nm: 'ahh', tone: 'amber', seed: 31, sel: true }] },
  ];
  return (
    <div className="tk-app">
      <TopBar bpm={92} gain={0.82} />
      <div className="tk-body">
        <LibraryPanel />
        <div className="tk-central">
          <div className="tk-tl-bar">
            <div className="tk-tl-transport">
              <button className="tk-round play" title="play">▶</button>
              <div className="tk-tnum" style={{ fontSize: 13, color: 'var(--ink)', marginLeft: 4 }}>00:03.2</div>
              <span style={{ fontSize: 10.5, color: 'var(--dim)' }}>· bar 4·1</span>
              <span style={{ fontSize: 10, letterSpacing: 2, color: 'var(--amber)', border: '1px solid var(--amber-ln)', padding: '2px 7px', borderRadius: 3, marginLeft: 12, textTransform: 'uppercase' }}>proposed</span>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              <div className="tk-stat tk-tnum" style={{ gap: 6 }}><span className="u">BPM</span><b>92</b></div>
              <button className="tk-btn ghost">＋ track</button>
              <button className="tk-btn amber">render → WAV</button>
            </div>
          </div>

          <div className="tk-tl-body">
            <div className="tk-tl-lanes">
              <div className="tk-lane-head">Tracks</div>
              {lanes.map((l) => (
                <div key={l.nm} className="tk-lane-lbl">
                  <span className="nm">{l.nm}</span>
                  <span className="ct">{l.ct}</span>
                </div>
              ))}
            </div>

            <div className="tk-tl-grid">
              <div className="tk-tl-ruler">
                {Array.from({ length: BARS }).map((_, i) => (
                  <div key={i} className="bar-tick" style={{ left: pc(i) }}>{i + 1}</div>
                ))}
              </div>
              <div className="tk-tl-playhead" style={{ left: pc(3.2) }} />
              {lanes.map((l, li) => (
                <div key={li} className="tk-lane">
                  {Array.from({ length: BARS }).map((_, i) => (
                    <div key={i} className="gridline" style={{ left: pc(i) }} />
                  ))}
                  {l.blocks.map((b, bi) => (
                    <div key={bi} className={'tk-block' + (b.tone === 'amber' ? ' amber' : '') + (b.sel ? ' sel' : '')} style={wb(b.a, b.b)}>
                      <span className="bl-nm">{b.nm}</span>
                      <div className="bl-wave"><Wave seed={b.seed} bars={Math.max(8, Math.round((b.b - b.a) * 14))} tone={b.tone === 'amber' ? 'amber' : 'green'} /></div>
                    </div>
                  ))}
                </div>
              ))}
              <div className="tk-lane filler">
                {Array.from({ length: BARS }).map((_, i) => (
                  <div key={i} className="gridline" style={{ left: pc(i) }} />
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── 5 · SESSION LOAD PICKER ─────────────────────────────────────────────────
function ScreenSession() {
  const sessions = [
    { fn: 'lastnight.tekr', sub: 'saved 2h ago', pads: '6 pads', tl: 'timeline · 8 bars', bpm: 92, sel: true },
    { fn: 'boombap_v3.tekr', sub: 'saved yesterday', pads: '8 pads', tl: 'no timeline', bpm: 88 },
    { fn: 'radio_edit.tekr', sub: 'missing 2 sources — will skip', pads: '7 pads', tl: 'timeline · 16 bars', bpm: 96, warn: true },
  ];
  return (
    <div className="tk-app">
      <TopBar bpm={92} gain={0.82} />
      <div className="tk-body">
        <LibraryPanel dim />
        <div className="tk-central">
          <div className="tk-session">
            <div className="tk-session-head">
              <div className="ti">
                <span className="tag">load</span>
                <h2>restore a session</h2>
              </div>
              <div className="dir">from <b>./</b> launch dir · <b>3</b> .tekr files</div>
            </div>

            <div className="tk-tekr-list">
              {sessions.map((s) => (
                <div key={s.fn} className={'tk-tekr' + (s.sel ? ' sel' : '')}>
                  <span className="fi">◉</span>
                  <div className="col">
                    <span className="fn">{s.fn}</span>
                    <span className={'sub' + (s.warn ? ' warn' : '')}>{s.sub}</span>
                  </div>
                  <div className="spec tk-tnum">
                    <div className="kv"><span className="k">pads</span><span className="v">{s.pads}</span></div>
                    <div className="kv"><span className="k">arrange</span><span className="v">{s.tl}</span></div>
                    <div className="kv"><span className="k">bpm</span><span className="v amber">{s.bpm}</span></div>
                  </div>
                </div>
              ))}
            </div>

            <div className="tk-session-foot">
              <span className="note">paths only — sources re-decode from disk on load · missing files flag <span style={{ color: 'var(--red)' }}>red</span> and skip</span>
              <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
                <button className="tk-btn ghost">cancel</button>
                <button className="tk-btn solid">load selected</button>
              </div>
            </div>
          </div>
        </div>
      </div>
      <ScratchPanel armed="scratch_ahh.wav" playheadPct={0} rotate={0} dim />
    </div>
  );
}

Object.assign(window, { ScreenMain, ScreenClip, ScreenScratch, ScreenTimeline, ScreenSession });
