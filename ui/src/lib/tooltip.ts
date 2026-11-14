/** How long the pointer rests on an element before its tooltip shows. */
export const TOOLTIP_DELAY_MS = 300;
/** After a tooltip hid, the next one within this time shows at once ("warm"). */
export const TOOLTIP_WARM_MS = 600;

/**
 * How long to wait before showing a tooltip: none while one is showing or one just hid (moving
 * along a toolbar shows each at once), else the resting delay.
 */
export function tooltipDelay(now: number, lastHidden: number | null, showing: boolean): number {
  if (showing) return 0;
  if (lastHidden !== null && now - lastHidden < TOOLTIP_WARM_MS) return 0;
  return TOOLTIP_DELAY_MS;
}
