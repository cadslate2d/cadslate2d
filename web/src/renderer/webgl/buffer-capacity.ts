/**
 * Capacity policy for GPU buffers (DESIGN §12.6): keep the allocation when the
 * data fits, otherwise grow geometrically so repeated small increases do not
 * reallocate every time.
 */
export function nextCapacity(current: number, needed: number, minimum = 1024): number {
  if (needed <= current) return current;
  return Math.max(needed, current * 2, minimum);
}
