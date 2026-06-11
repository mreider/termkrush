// tk-app.jsx — lay the TermKrush auto-mixer states out on the design canvas
const { DesignCanvas, DCSection, DCArtboard, DCPostIt } = window;
const { ScreenMain, ScreenTapping, ScreenReady, ScreenRendering, ScreenComplete, ScreenNeeds, ScreenEmpty } = window;

const W = 1280, H = 800;

function App() {
  return (
    <DesignCanvas>
      <DCSection id="flow" title="The mix, end to end" subtitle="One window · three surfaces · zero knobs — Library (left) · Beat-tap (center) · Sequence line (bottom)">
        <DCArtboard id="main" label="Main view — at rest" width={W} height={H}><ScreenMain /></DCArtboard>
        <DCArtboard id="tapping" label="Beat-tap — tapping ↓ (live fit)" width={W} height={H}><ScreenTapping /></DCArtboard>
        <DCArtboard id="ready" label="Sequence — ready to render" width={W} height={H}><ScreenReady /></DCArtboard>
        <DCArtboard id="rendering" label="Render — in progress" width={W} height={H}><ScreenRendering /></DCArtboard>
        <DCArtboard id="complete" label="Render — complete" width={W} height={H}><ScreenComplete /></DCArtboard>
      </DCSection>

      <DCSection id="edges" title="Edge states" subtitle="Where the engine guides you — what 'curate, it executes' looks like before everything's ready">
        <DCArtboard id="needs" label="A sequenced track needs beats" width={W} height={H}><ScreenNeeds /></DCArtboard>
        <DCArtboard id="empty" label="Empty / first run" width={W} height={H}><ScreenEmpty /></DCArtboard>
        <DCPostIt top={40} left={W + 70} width={250} rotate={2}>
          The only two inputs are track order and tapped beats. Render is disabled until every chip in the sequence has a grid — the sequence line tells you exactly which tracks still need beats.
        </DCPostIt>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
