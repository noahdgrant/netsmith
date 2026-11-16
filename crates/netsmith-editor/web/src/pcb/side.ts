// The side of the board the PCB view looks at: the front, the back (the board as seen from the back, mirrored left to right as KiCad's
// flipped view is, so a part on the back reads as it is on the real board), or both faces as they are in the file.

export type Side = 'front' | 'back' | 'both';

export const SIDES: Side[] = ['front', 'back', 'both'];

/** The side after `side`, for the key. */
export function nextSide(side: Side): Side {
  return SIDES[(SIDES.indexOf(side) + 1) % SIDES.length];
}

/** The face of the board a part is on, in the words the page uses: the file says `top` and `bottom`, the board has a front and a back. */
export function faceWord(side: 'top' | 'bottom'): 'front' | 'back' {
  return side === 'bottom' ? 'back' : 'front';
}

/** Whether a keepout or a height limit on the copper layers `layers` is drawn when `side` is looked at: `both` shows every zone, the front
 * those that apply to the top layer and the back those that apply to the bottom layer (a zone of inner layers only is in neither). */
export function appliesTo(layers: readonly string[], side: Side): boolean {
  if (side === 'both') return true;
  return layers.includes(side === 'front' ? 'top' : 'bottom');
}
