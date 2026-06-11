// tk-shared.jsx — CRT-egui building blocks for TermKrush (deterministic auto-mixer)
// Three surfaces: Library (left) · Beat-tap (center) · Sequence line (bottom)
// Exports: Vinyl, IconBtn, TopBar, Wave, LibraryPanel, BeatTapStage, Coach,
//          SequenceLine, libBase, seqReady, seqNeeds

// ---- deterministic pseudo-waveform ----------------------------------------
function tkRng(seed) {
  let s = seed % 2147483647; if (s <= 0) s += 2147483646;
  return () => (s = (s * 16807) % 2147483647) / 2147483647;
}
function tkWaveArr(seed, n) {
  const r = tkRng(seed);
  const out = [];
  for (let i = 0; i < n; i++) {
    const env = 0.4 + 0.6 * Math.abs(Math.sin((i / n) * Math.PI * 3 + seed));
    const h = (0.16 + 0.84 * r()) * env;
    out.push(Math.max(0.05, Math.min(1, h)));
  }
  return out;
}

function Wave({ seed = 7, bars = 80, tone = 'plain', style }) {
  const hs = React.useMemo(() => tkWaveArr(seed, bars), [seed, bars]);
  return (
    <div className={'tk-wave' + (tone === 'amber' ? ' amber' : tone === 'green' ? ' green' : '')} style={style}>
      {hs.map((h, i) => <div key={i} className="bar" style={{ height: (h * 100) + '%' }} />)}
    </div>
  );
}

function Vinyl({ spin }) { return <span className={'tk-vinyl' + (spin ? ' spin' : '')} aria-hidden="true" />; }

function IconBtn({ children, amber, title }) {
  return <button className={'tk-icon' + (amber ? ' amber' : '')} title={title}>{children}</button>;
}

// ---- top bar: status only (zero knobs) ------------------------------------
function TopBar({ masterBpm = null, masterFrom = '', crate = '~/Music/termkrush' }) {
  return (
    <div className="tk-top">
      <div className="tk-logo">
        <Vinyl />
        <span className="tk-word">termkrush</span>
      </div>
      <div className="tk-topright">
        <div className="tk-crate"><span className="ic">▣</span> crate <b>{crate}</b></div>
        <div className="tk-sep" />
        <div className={'tk-grid-read' + (masterBpm ? '' : ' unset')}>
          <span className="lk">{masterBpm ? 'grid locked' : 'no grid'}</span>
          <span className="bpm tk-tnum"><b>{masterBpm || '—'}</b><span className="u">bpm master{masterFrom ? ' · ' + masterFrom : ''}</span></span>
        </div>
      </div>
    </div>
  );
}

// ---- library ---------------------------------------------------------------
// each: {name, len, bpm|null, folder, open, indent}
const libBase = [
  { folder: true, name: 'crate', open: true },
  { name: 'track_01.wav', len: '3:42', bpm: 92, indent: true },
  { name: 'track_02.wav', len: '4:05', bpm: 88, indent: true },
  { name: 'track_03.wav', len: '2:58', bpm: 124, indent: true },
  { name: 'track_04.wav', len: '5:11', bpm: null, indent: true },
  { name: 'track_05.mp3', len: '3:30', bpm: 96, indent: true },
  { folder: true, name: 'imports', open: true, indent: true },
  { name: 'track_06.mp3', len: '4:20', bpm: null, indent: true, deep: true },
  { name: 'track_07.wav', len: '3:05', bpm: 100, indent: true, deep: true },
];

// LibraryPanel options:
//   editing   — track name currently open in beat-tap (amber highlight)
//   flag      — array of names to mark amber "needs beats"
//   fresh     — {name, len} rendered output row pinned at top
//   allRaw    — true: nothing tapped yet (first-run) — every track shows "needs beats"
//   dim       — fade the whole panel
function LibraryPanel({ editing, flag = [], fresh, allRaw, dim }) {
  const rows = [];
  if (fresh) rows.push({ fresh: true, ...fresh });
  libBase.forEach((r) => rows.push(r));

  return (
    <div className={'tk-side' + (dim ? ' tk-dim' : '')}>
      <div className="tk-phead">
        <span>Library</span>
        <div className="tk-tools">
          <IconBtn title="new folder">＋</IconBtn>
          <IconBtn title="preview ▶">▶</IconBtn>
          <IconBtn title="trash">🗑</IconBtn>
        </div>
      </div>
      <div className="tk-tree">
        {rows.map((r, i) => {
          if (r.fresh) {
            return (
              <div key={'f' + i} className="tk-row fresh tk-indent">
                <span className="tw">♪</span>
                <span className="nm">{r.name}</span>
                <span className="tag">new mix</span>
              </div>
            );
          }
          if (r.folder) {
            return (
              <div key={i} className={'tk-row folder' + (r.indent ? ' tk-indent' : '')}>
                <span className="tw">{r.open ? '▾' : '▸'}</span>
                <span className="nm">{r.name}/</span>
              </div>
            );
          }
          const raw = allRaw || r.bpm == null;
          const flagged = flag.includes(r.name);
          const isEditing = editing === r.name;
          const pad = r.deep ? { paddingLeft: 40 } : undefined;
          return (
            <div key={i}
              className={'tk-row' + (r.indent ? ' tk-indent' : '') + (isEditing ? ' editing' : '') + (flagged ? ' flag' : '')}
              style={pad}>
              <span className="tw">♪</span>
              <span className="nm">{r.name}</span>
              {raw
                ? <span className="needs"><span>needs beats</span><span className="pencil">✎</span></span>
                : <span className="bpm tk-tnum">{r.bpm}<span style={{ fontSize: 8, letterSpacing: .5, opacity: .7 }}> BPM</span></span>}
              {!raw && <span className="len tk-tnum">{r.len}</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---- beat-tap stage (center) ----------------------------------------------
// BeatTapStage props:
//   fn        — track filename
//   seed      — waveform seed
//   bpm       — fitted tempo (number)
//   tone      — 'green' (saved/locked) | 'amber' (provisional, mid-tap)
//   region    — [in,out] trim fractions
//   playhead  — playhead fraction
//   beats     — {start, count} grid spec
//   taps      — array of {pct, ghost} user tap carets (mid-tap)
//   tapping   — bool: show pulsing ↓ key + live fit; else "saved" footer
//   fit       — {taps, jitter, conf} live readout numbers
//   duration  — display string for total time
function BeatTapStage({ fn, seed = 11, bpm = 92, tone = 'green', region = [0.14, 0.82],
                        playhead = 0.2, beats = { start: 0.06, count: 25 }, taps = [],
                        tapping = false, fit = {}, duration = '3:42' }) {
  // spread an evenly-spaced beat grid across the trim region
  const span = region[1] - region[0];
  const step = span / beats.count;
  const marks = [];
  for (let i = 0; i <= beats.count; i++) {
    const down = i % 4 === 0;
    marks.push({ pct: (region[0] + i * step) * 100, down, bar: Math.floor(i / 4) + 1 });
  }
  const beatCls = tone === 'amber' ? 'tk-beat amber' : 'tk-beat';

  return (
    <div className="tk-tap">
      <div className="tk-tap-head">
        <span className="tag">{tapping ? 'tap beats' : 'beats'}</span>
        <span className="fn">{fn}</span>
        <span style={{ fontSize: 11, color: 'var(--dim)' }}>· trim is non-destructive · tapped once, kept forever</span>
        <span className="time tk-tnum">play <b>0:38</b> / <b>{duration}</b></span>
      </div>

      <div className="tk-tap-box">
        <div className="tk-ruler">
          {marks.filter((m) => m.down).map((m, i) => (
            <span key={i} className="barlab" style={{ left: m.pct + '%' }}>{m.bar}</span>
          ))}
        </div>

        {/* region shade outside trim */}
        <div className="tk-region-shade" style={{ left: 0, width: (region[0] * 100) + '%' }} />
        <div className="tk-region-shade" style={{ right: 0, width: ((1 - region[1]) * 100) + '%' }} />

        {/* waveform */}
        <div className="tk-wave-region"><Wave seed={seed} bars={180} tone="plain" /></div>

        {/* beat grid */}
        {marks.map((m, i) => (
          <div key={i} className={beatCls + (m.down ? ' down' : '')} style={{ left: m.pct + '%' }}>
            {m.down && <span className="cap">{m.bar}</span>}
          </div>
        ))}

        {/* trim handles */}
        <div className="tk-handle in" style={{ left: (region[0] * 100) + '%' }}><div className="grab">◀</div></div>
        <div className="tk-handle out" style={{ left: (region[1] * 100) + '%' }}><div className="grab">▶</div></div>

        {/* playhead */}
        <div className="tk-playhead" style={{ left: (playhead * 100) + '%' }} />

        {/* tap lane */}
        <div className="tk-tap-lane">
          <span className="lbl">↓ taps</span>
          {taps.map((t, i) => (
            <span key={i} className={'tk-tap-caret' + (t.ghost ? ' ghost' : '')} style={{ left: t.pct + '%' }}>▾</span>
          ))}
        </div>
      </div>

      <div className="tk-tap-foot">
        <div className="tk-fit">
          <div className="kv">
            <span className="k">tempo</span>
            <span className={'v tk-tnum ' + (tone === 'amber' ? 'amber' : 'green')}>{bpm}<small> bpm</small></span>
          </div>
          <div className="kvsep" />
          <div className="kv">
            <span className="k">downbeat</span>
            <span className="v tk-tnum">{fit.downbeat || '0.182'}<small> s</small></span>
          </div>
          <div className="kvsep" />
          <div className="kv">
            <span className="k">taps fit</span>
            <span className="v tk-tnum">{fit.taps != null ? fit.taps : 16}</span>
          </div>
          <div className="kvsep" />
          <div className="kv">
            <span className="k">residual</span>
            <span className="v tk-tnum">±{fit.jitter || '3'}<small> ms</small></span>
          </div>
          <div className="live">
            {tapping
              ? <><span className="pill"><span className="dot" /> fitting live</span><span className="hint">keep tapping — least-squares averages every ↓</span></>
              : <><span className="pill" style={{ color: 'var(--green)' }}>✓ saved to beats.txt</span><span className="hint">marks follow renames &amp; moves</span></>}
          </div>
        </div>

        <div className="tk-tap-ctl">
          {tapping
            ? <div className="tk-tapkey armed"><span className="glyph">↓</span><span className="lbl">tap on beat</span></div>
            : <div className="tk-tapkey idle"><span className="glyph">↓</span><span className="lbl">tap to re-fit</span></div>}
          <div className="btnrow">
            <button className="tk-btn"><span className="play">▶</span> play</button>
            <button className={'tk-btn ' + (tapping ? 'amber' : 'green')}>{tapping ? 'save' : 'saved ✓'}</button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---- empty / first-run coach ----------------------------------------------
function Coach() {
  return (
    <div className="tk-coach">
      <div className="arrow">↓</div>
      <div className="big">tap a beat, once</div>
      <p>Pick a track in the library and hit the <b>pencil</b>. Play it, tap the <b>↓ arrow</b> on each beat — TermKrush fits an exact tempo and downbeat. You tap each track <b>once, ever</b>.</p>
      <div className="steps">
        <div className="step"><span className="n">1</span>tap beats</div>
        <div className="step"><span className="n">2</span>line up tracks</div>
        <div className="step"><span className="n">3</span>render</div>
      </div>
    </div>
  );
}

// ---- sequence line (bottom) -----------------------------------------------
const seqReady = [
  { name: 'track_01.wav', bpm: 92, master: true, seed: 11 },
  { name: 'track_03.wav', bpm: 124, seed: 5 },
  { name: 'track_01.wav', bpm: 92, seed: 11 },
  { name: 'track_05.mp3', bpm: 96, seed: 23 },
  { name: 'track_02.wav', bpm: 88, seed: 31 },
  { name: 'track_03.wav', bpm: 124, seed: 5 },
];
const seqNeeds = [
  { name: 'track_01.wav', bpm: 92, master: true, seed: 11 },
  { name: 'track_04.wav', bpm: null, seed: 7 },
  { name: 'track_05.mp3', bpm: 96, seed: 23 },
  { name: 'track_06.mp3', bpm: null, seed: 17 },
];

function Chip({ item, n }) {
  const flag = item.bpm == null;
  return (
    <div className={'tk-chip' + (item.master ? ' master' : '') + (flag ? ' flag' : '')}>
      <div className="tk-chip-head">
        <span className="ord">{n}</span>
        <span className="cn">{item.name}</span>
        <span className="x" title="remove">✕</span>
      </div>
      <div className="tk-chip-foot">
        {flag
          ? <span className="needbadge" title="open beat-tap">needs beats ✎</span>
          : <span className="tempo tk-tnum">{item.bpm} BPM</span>}
        {item.master && <span className="sets">sets tempo</span>}
      </div>
      <div className="mini"><Wave seed={item.seed} bars={26} tone={flag ? 'plain' : 'green'} /></div>
    </div>
  );
}

// SequenceLine props:
//   items   — chip array
//   status  — 'ready' | 'wait' | 'empty'
//   working — render in progress {pct, phase:[...done], now, label}
//   needCount — how many tracks need beats (for wait status)
function SequenceLine({ items = [], status = 'ready', working = null, needCount = 0, masterBpm = 92 }) {
  return (
    <div className="tk-seqline">
      <div className="tk-seq-head">
        <span className="ti">Sequence</span>
        {status === 'ready' && !working && <span className="tk-status ready"><span className="ic">✓</span> ready to render</span>}
        {status === 'wait' && !working && <span className="tk-status wait">◷ {needCount} track{needCount > 1 ? 's' : ''} need beats</span>}
        {status === 'empty' && !working && <span className="tk-status empty">empty — drag tracks in to start</span>}
        {working && <span className="tk-status wait"><Vinyl spin /> rendering…</span>}
        <span className="autosave"><span className="d" /> autosaved · sequence.txt</span>
        <div className="right">
          <span className={'tk-master-mini' + (masterBpm ? '' : ' unset')}>master <b>{masterBpm || '—'}</b> BPM</span>
          {working
            ? <span className="tk-render working"><Vinyl spin /> rendering {working.pct}%</span>
            : status === 'ready'
              ? <button className="tk-render"><span className="ic">▶</span> render mix</button>
              : <button className="tk-render disabled"><span className="ic">▶</span> render mix</button>}
        </div>
      </div>

      {working && <div className="tk-progress"><div className="fill" style={{ width: working.pct + '%' }} /></div>}

      <div className="tk-seq-track">
        {items.length === 0
          ? <div className="tk-drop big"><span className="plus">＋</span><b>drag tracks here to build your mix</b><span>the first track sets the master tempo · repeats welcome</span></div>
          : <>
              {items.map((it, i) => (
                <React.Fragment key={i}>
                  {i > 0 && <span className="tk-swap">›</span>}
                  <Chip item={it} n={i + 1} />
                </React.Fragment>
              ))}
              <span className="tk-swap">›</span>
              <div className="tk-drop"><span className="plus">＋</span><b>drop</b></div>
            </>}
      </div>
    </div>
  );
}

Object.assign(window, {
  Vinyl, IconBtn, TopBar, Wave,
  LibraryPanel, BeatTapStage, Coach, SequenceLine, Chip,
  libBase, seqReady, seqNeeds,
});
