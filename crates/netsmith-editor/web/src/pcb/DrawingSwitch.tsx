// The first group of the PCB view's bar: the board in 2D, in 3D, or the Gerbers of its fab package (issue 1107). It is
// where it was whichever drawing is shown (issue 784).

import { Segmented } from '../components/ui';
import { type PcbMode, pcbMode } from '../pcb3d/mode';

export function DrawingSwitch() {
  return (
    <div class="tb-group">
      <Segmented<PcbMode>
        label="Drawing"
        value={pcbMode.value}
        onChange={(v) => (pcbMode.value = v)}
        options={[
          { value: '2d', label: '2D', tip: 'The board in 2D, where parts are placed and moved', keys: 'D', testid: 'pcb-2d' },
          { value: '3d', label: '3D', icon: 'cube', tip: 'The board in 3D, with the heights netsmith knows: drag a picked part to move it', keys: 'D', testid: 'pcb-3d-toggle' },
          {
            value: 'gerber',
            label: 'Gerber',
            icon: 'layers',
            tip: "The Gerber and drill files of the board's fab package, as the board house gets them: read and drawn by netsmith's own reader, as `netsmith view gerber` draws them",
            testid: 'pcb-gerber',
          },
        ]}
      />
    </div>
  );
}
