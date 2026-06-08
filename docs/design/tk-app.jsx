// tk-app.jsx — lay the five TermKrush screens out on the design canvas
const { DesignCanvas, DCSection, DCArtboard, DCPostIt } = window;
const { ScreenMain, ScreenClip, ScreenScratch, ScreenTimeline, ScreenSession } = window;

const W = 1280, H = 800;

function App() {
  return (
    <DesignCanvas>
      <DCSection id="core" title="Core — built ✅" subtitle="The finished surfaces: library, pads, clip editor, live scratch">
        <DCArtboard id="main" label="Main view" width={W} height={H}><ScreenMain /></DCArtboard>
        <DCArtboard id="clip" label="Clip editor" width={W} height={H}><ScreenClip /></DCArtboard>
        <DCArtboard id="scratch" label="Scratch — armed / focused" width={W} height={H}><ScreenScratch /></DCArtboard>
        <DCArtboard id="session" label="Session load (.tekr)" width={W} height={H}><ScreenSession /></DCArtboard>
      </DCSection>

      <DCSection id="backlog" title="Backlog — proposed 🔜" subtitle="The GUI free-track timeline editor, built on the finished arrangement model">
        <DCArtboard id="timeline" label="Timeline editor" width={W} height={H}><ScreenTimeline /></DCArtboard>
        <DCPostIt top={40} left={W + 70} width={230} rotate={2}>
          Open Qs for the PM: snap blocks to the bar grid or free placement? does playback route through the mixer or sum alongside? where does paste land?
        </DCPostIt>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
