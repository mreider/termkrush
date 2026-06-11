// tk-screens.jsx — TermKrush states for the deterministic auto-mixer.
// One window, three surfaces: Library (left) · Beat-tap (center) · Sequence line (bottom).
const { TopBar, LibraryPanel, BeatTapStage, Coach, SequenceLine } = window;
const { seqReady, seqNeeds } = window;

// ── 1 · MAIN VIEW — all three surfaces at rest ──────────────────────────────
function ScreenMain() {
  return (
    <div className="tk-app">
      <TopBar masterBpm={92} masterFrom="track_01" />
      <div className="tk-body">
        <LibraryPanel editing="track_01.wav" />
        <div className="tk-central">
          <BeatTapStage
            fn="track_01.wav" seed={11} bpm={92} tone="green"
            region={[0.12, 0.84]} playhead={0.21} beats={{ start: 0.12, count: 22 }}
            fit={{ taps: 18, jitter: '3', downbeat: '0.182' }} duration="3:42" />
        </div>
      </div>
      <SequenceLine items={seqReady} status="ready" masterBpm={92} />
    </div>
  );
}

// ── 2 · BEAT-TAP IN PROGRESS — tapping ↓, live fit ──────────────────────────
function ScreenTapping() {
  // taps scattered just off the forming grid — proof it's a least-squares fit
  const taps = [
    { pct: 16 }, { pct: 23.5 }, { pct: 31 }, { pct: 39 },
    { pct: 46 }, { pct: 54.5 }, { pct: 61.5 }, { pct: 69, ghost: true },
  ];
  return (
    <div className="tk-app">
      <TopBar masterBpm={92} masterFrom="track_01" />
      <div className="tk-body">
        <LibraryPanel editing="track_04.wav" />
        <div className="tk-central">
          <BeatTapStage
            fn="track_04.wav" seed={7} bpm="118.4" tone="amber"
            region={[0.1, 0.86]} playhead={0.62} beats={{ start: 0.1, count: 24 }}
            taps={taps} tapping fit={{ taps: 8, jitter: '11', downbeat: '0.094' }} duration="5:11" />
        </div>
      </div>
      <SequenceLine items={seqNeeds} status="wait" needCount={2} masterBpm={92} />
    </div>
  );
}

// ── 3 · READY TO RENDER ─────────────────────────────────────────────────────
function ScreenReady() {
  return (
    <div className="tk-app">
      <TopBar masterBpm={92} masterFrom="track_01" />
      <div className="tk-body">
        <LibraryPanel editing="track_01.wav" />
        <div className="tk-central">
          <BeatTapStage
            fn="track_03.wav" seed={5} bpm={124} tone="green"
            region={[0.16, 0.8]} playhead={0.3} beats={{ start: 0.16, count: 20 }}
            fit={{ taps: 24, jitter: '2', downbeat: '0.061' }} duration="2:58" />
        </div>
      </div>
      <SequenceLine items={seqReady} status="ready" masterBpm={92} />
    </div>
  );
}

// ── 4 · RENDER IN PROGRESS ──────────────────────────────────────────────────
function ScreenRendering() {
  return (
    <div className="tk-app">
      <TopBar masterBpm={92} masterFrom="track_01" />
      <div className="tk-body">
        <LibraryPanel editing="track_01.wav" dim />
        <div className="tk-central">
          <div className="tk-render-panel">
            <div className="vinyl-lg" />
            <div className="title">rendering mix</div>
            <div className="meter">
              <div className="bar"><div className="fill" style={{ width: '62%' }} /></div>
              <div className="row tk-tnum">
                <span>phrase <b>5</b> of 8 · seamless swap on bar 33</span>
                <span><b>62%</b> · ~14s left</span>
              </div>
            </div>
            <div className="phases">
              <span className="phase done">decode</span>
              <span className="phase done">varispeed → 92</span>
              <span className="phase done">grid fit</span>
              <span className="phase now">arrange + scratch</span>
              <span className="phase">bounce WAV</span>
            </div>
            <p style={{ fontSize: 11, color: 'var(--dim)', maxWidth: 460, textAlign: 'center', lineHeight: 1.7 }}>
              Everything varispeeds to <span style={{ color: 'var(--amber)' }}>92 BPM</span> — the grid set by track_01 and never moves.
              Scratches and fader chops start on the grid, stay loose inside. Seeded, so this render is bit-for-bit reproducible.
            </p>
          </div>
        </div>
      </div>
      <SequenceLine items={seqReady} status="ready" working={{ pct: 62 }} masterBpm={92} />
    </div>
  );
}

// ── 5 · RENDER COMPLETE ─────────────────────────────────────────────────────
function ScreenComplete() {
  return (
    <div className="tk-app">
      <TopBar masterBpm={92} masterFrom="track_01" />
      <div className="tk-body">
        <LibraryPanel editing={null} fresh={{ name: 'mix_07.wav', len: '47:50' }} />
        <div className="tk-central">
          <div className="tk-done-panel">
            <div className="check">✓</div>
            <div className="title">mix rendered</div>
            <div className="file">mix_07.wav</div>
            <div className="specs tk-tnum">
              <div className="kv"><div className="k">length</div><div className="v">47:50</div></div>
              <div className="kv"><div className="k">tempo</div><div className="v amber">92 BPM</div></div>
              <div className="kv"><div className="k">phrases</div><div className="v">8</div></div>
              <div className="kv"><div className="k">format</div><div className="v">WAV · 48k</div></div>
            </div>
            <div className="repro">
              <span>same sequence → same mix, bit for bit</span>
              <span className="seedtag tk-tnum">seed 0x5f3a</span>
            </div>
            <div className="actions">
              <button className="tk-btn green"><span className="play">▶</span> play mix</button>
              <button className="tk-btn">⤓ reveal in library</button>
              <button className="tk-btn ghost">export → MP3</button>
            </div>
          </div>
        </div>
      </div>
      <SequenceLine items={seqReady} status="ready" masterBpm={92} />
    </div>
  );
}

// ── 6 · NEEDS BEATS — a sequenced track has no grid ─────────────────────────
function ScreenNeeds() {
  return (
    <div className="tk-app">
      <TopBar masterBpm={92} masterFrom="track_01" />
      <div className="tk-body">
        <LibraryPanel editing="track_01.wav" flag={['track_04.wav', 'track_06.mp3']} />
        <div className="tk-central">
          <BeatTapStage
            fn="track_01.wav" seed={11} bpm={92} tone="green"
            region={[0.12, 0.84]} playhead={0.21} beats={{ start: 0.12, count: 22 }}
            fit={{ taps: 18, jitter: '3', downbeat: '0.182' }} duration="3:42" />
        </div>
      </div>
      <SequenceLine items={seqNeeds} status="wait" needCount={2} masterBpm={92} />
    </div>
  );
}

// ── 7 · EMPTY / FIRST RUN — nothing tapped, no sequence ─────────────────────
function ScreenEmpty() {
  return (
    <div className="tk-app">
      <TopBar masterBpm={null} />
      <div className="tk-body">
        <LibraryPanel allRaw />
        <div className="tk-central">
          <Coach />
        </div>
      </div>
      <SequenceLine items={[]} status="empty" masterBpm={null} />
    </div>
  );
}

Object.assign(window, {
  ScreenMain, ScreenTapping, ScreenReady, ScreenRendering, ScreenComplete, ScreenNeeds, ScreenEmpty,
});
