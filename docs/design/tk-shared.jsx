// tk-shared.jsx — shared CRT-egui building blocks for the TermKrush interface
// Exports to window: Vinyl, TopBar, IconBtn, Seg, Slider, Toggle, Wave,
//                    LibraryPanel, Pad, ScratchPanel, padData, libData

// ---- deterministic pseudo-waveform ----------------------------------------
function tkRng(seed) {
  let s = seed % 2147483647; if (s <= 0) s += 2147483646;
  return () => (s = (s * 16807) % 2147483647) / 2147483647;
}
// returns array of bar heights (0..1), min/max style (centered)
function tkWave(seed, n) {
  const r = tkRng(seed);
  const out = [];
  for (let i = 0; i < n; i++) {
    const env = 0.45 + 0.55 * Math.abs(Math.sin((i / n) * Math.PI * 3 + seed));
    const h = (0.18 + 0.82 * r()) * env;
    out.push(Math.max(0.06, Math.min(1, h)));
  }
  return out;
}

// Wave: a row of centered bars. `region` = [a,b] fractions -> amber selection,
// rest dim. `tone` = 'amber' | 'green' | 'plain'
function Wave({ seed = 7, bars = 80, region = null, tone = 'plain', style }) {
  const hs = React.useMemo(() => tkWave(seed, bars), [seed, bars]);
  return (
    <div className={'tk-wave' + (tone === 'amber' && !region ? ' amber' : '')} style={style}>
      {hs.map((h, i) => {
        let cls = 'bar';
        if (region) {
          const f = i / bars;
          if (f >= region[0] && f <= region[1]) cls += tone === 'green' ? ' gsel' : ' sel';
          else cls += ' dim';
        } else if (tone === 'green') cls += ' gsel';
        return <div key={i} className={cls} style={{ height: (h * 100) + '%' }} />;
      })}
    </div>
  );
}

function Vinyl() { return <span className="tk-vinyl" aria-hidden="true" />; }

function IconBtn({ children, amber, title }) {
  return <button className={'tk-icon' + (amber ? ' amber' : '')} title={title}>{children}</button>;
}

// ---- top transport bar -----------------------------------------------------
function TopBar({ bpm = 92, gain = 0.82, paused = false }) {
  return (
    <div className="tk-top">
      <div className="tk-logo">
        <Vinyl />
        <span className="tk-word">termkrush</span>
      </div>
      <div className="tk-transport">
        <div className="tk-stat tk-tnum"><span className="u">BPM</span><b>{bpm}</b></div>
        <IconBtn title="tempo down">−</IconBtn>
        <IconBtn title="tempo up">+</IconBtn>
        <div className="tk-sep" />
        <span className="tk-stat" style={{ gap: 8 }}><span className="u">MASTER</span></span>
        <div className="tk-srow" style={{ width: 110 }}>
          <Slider value={gain} />
        </div>
        <div className="tk-sep" />
        <IconBtn amber={paused} title="master pause">{paused ? '▶' : '⏸'}</IconBtn>
        <IconBtn title="load session (L)">⤓</IconBtn>
      </div>
    </div>
  );
}

// ---- widgets ---------------------------------------------------------------
function Seg({ value, options }) {
  return (
    <div className="tk-seg">
      {options.map((o) => (
        <button key={o} className={o === value ? 'act' : ''}>{o}</button>
      ))}
    </div>
  );
}

function Slider({ value = 0.5, green }) {
  const pct = Math.round(value * 100);
  return (
    <div className={'tk-slider' + (green ? ' green' : '')}>
      <div className="trk" />
      <div className="fil" style={{ width: pct + '%' }} />
      <div className="knb" style={{ left: pct + '%' }} />
    </div>
  );
}

function Toggle({ on, label }) {
  return (
    <div className={'tk-toggle' + (on ? ' on' : '')}>
      <span className="tk-sw"><span className="kn" /></span>
      <span className="tl">{label || (on ? 'ON' : 'OFF')}</span>
    </div>
  );
}

// ---- library ---------------------------------------------------------------
const libData = [
  { kind: 'folder', name: 'crates', open: true },
  { kind: 'folder', name: 'breaks', indent: true },
  { kind: 'folder', name: '808s', indent: true },
  { kind: 'file', name: 'amen_break.wav', meta: '6.4s', indent: true, sel: true },
  { kind: 'file', name: 'vinyl_crackle.wav', meta: '8.0s', indent: true },
  { kind: 'file', name: 'rhodes_stab.mp3', meta: '1.2s', indent: true },
  { kind: 'file', name: 'scratch_ahh.wav', meta: '0.9s', indent: true },
  { kind: 'file', name: 'kick_909.wav', meta: '0.4s', indent: true },
  { kind: 'file', name: 'hat_loop.wav', meta: '2.0s', indent: true },
  { kind: 'file', name: 'corrupt_take.mp3', bad: true, indent: true },
];

function LibraryPanel({ dim }) {
  return (
    <div className={'tk-side' + (dim ? ' tk-dim' : '')}>
      <div className="tk-phead">
        <span>Library</span>
        <div className="tk-tools">
          <IconBtn title="new folder">＋</IconBtn>
          <IconBtn title="up one level">⬆</IconBtn>
          <IconBtn title="preview">▶</IconBtn>
          <IconBtn title="delete">🗑</IconBtn>
        </div>
      </div>
      <div className="tk-tree">
        {libData.map((row, i) => {
          if (row.kind === 'folder') {
            return (
              <div key={i} className={'tk-row folder' + (row.indent ? ' tk-indent' : '')}>
                <span className="tw">{row.open ? '▾' : '▸'}</span>
                <span className="nm">{row.name}/</span>
              </div>
            );
          }
          return (
            <div key={i} className={'tk-row' + (row.indent ? ' tk-indent' : '') + (row.sel ? ' sel' : '') + (row.bad ? ' bad' : '')}>
              <span className="tw">♪</span>
              <span className="nm">{row.name}</span>
              {row.bad ? <span className="tk-badge-bad">no codec</span> : <span className="meta tk-tnum">{row.meta}</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---- pads ------------------------------------------------------------------
// kind: 1shot | loop | scratch ; state per pad
const padData = [
  { n: 1, name: 'amen_break', kind: 'loop', on: true, vol: 0.82, playing: true, seed: 11 },
  { n: 2, name: 'vinyl_crackle', kind: 'loop', on: true, vol: 0.30, playing: true, seed: 5 },
  { n: 3, name: 'rhodes_stab', kind: '1shot', on: true, vol: 0.62, playing: false, seed: 23 },
  { n: 4, name: 'scratch_ahh', kind: 'scratch', on: true, vol: 0.70, playing: false, seed: 31 },
  { n: 5, name: 'kick_909', kind: '1shot', on: false, vol: 0.55, playing: false, seed: 8 },
  { n: 6, empty: true, n6: true },
  { n: 7, name: 'hat_loop', kind: 'loop', on: true, vol: 0.48, playing: true, seed: 17 },
  { n: 8, empty: true },
];

function Pad({ p }) {
  if (p.empty) {
    return (
      <div className="tk-pad empty">
        <div className="tk-pnum" style={{ position: 'absolute', top: 8, left: 9 }}>{p.n}</div>
        <div className="tk-pad-hint">drag a track<br /><span className="k">here</span></div>
      </div>
    );
  }
  const cls = 'tk-pad' + (p.on ? ' on' : '') + (p.kind === 'scratch' ? ' scratch' : '');
  return (
    <div className={cls}>
      <div className="tk-pad-head">
        <span className="tk-pnum">{p.n}</span>
        <span className="tk-pname">{p.name}</span>
        <button className="tk-icon" title={p.playing ? 'pause' : 'play'} style={{ color: p.playing ? 'var(--green)' : 'var(--dim)' }}>{p.playing ? '⏸' : '▶'}</button>
      </div>
      <div className="tk-pad-body">
        <Seg value={p.kind} options={['1shot', 'loop', 'scratch']} />
        <div className="tk-srow">
          <span className="lab">VOL</span>
          <Slider value={p.vol} />
          <span className="val tk-tnum">{Math.round(p.vol * 100)}</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 'auto' }}>
          <Toggle on={p.on} />
          <div className="tk-btnrow">
            <button className="tk-btn ghost" title="clear">clr</button>
            <button className="tk-btn ghost" title="export trimmed clip">exp</button>
            <button className="tk-btn" title="edit clip">edit</button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---- bottom scratch platter ------------------------------------------------
// `active` tilts the platter + offsets the playhead to read as mid-scratch
function ScratchPanel({ active = false, armed = 'scratch_ahh.wav', playheadPct = 38, rotate = -18, dim = false }) {
  return (
    <div className={'tk-bottom' + (dim ? ' tk-dim' : '')}>
      <div className="tk-phead"><span>Scratch</span></div>
      <div className="tk-scratch">
        <div className="tk-deck">
          <div className="tk-platter" style={{ transform: `rotate(${rotate}deg)` }}>
            <span className="marker" />
          </div>
          <span className="arm">{armed ? 'armed' : 'drag a track to arm'}</span>
        </div>
        <div className="tk-scratch-main">
          <div className="tk-scratch-head">
            <div className="src">
              <span className="dot" />
              <span className="nm">{armed}</span>
              {active && <span style={{ color: 'var(--amber)', fontSize: 10.5, letterSpacing: 1 }}>▶ WIKI · fwd</span>}
            </div>
            <div className="hint">
              <span>drag platter ↔ to scratch</span>
              <span>hold <span className="key">←</span> <span className="key">→</span> to jog</span>
            </div>
          </div>
          <div className="tk-wavebox">
            <div className="tk-playhead" style={{ left: playheadPct + '%' }} />
            <div style={{ position: 'absolute', inset: '8px 0', padding: '0 2px' }}>
              <Wave seed={31} bars={140} tone="plain" />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  Wave, Vinyl, IconBtn, TopBar, Seg, Slider, Toggle,
  LibraryPanel, Pad, ScratchPanel, padData, libData,
});
