// The geometry of the sidebar's drag and drop, without the DOM: where a row
// goes and how far the other rows move aside. Positions are in pixels from
// the top of the sidebar, taken before the drag (transforms do not count).

export type Row = { id: string; top: number; height: number };

/**
 * The place of a row that is being moved, among the other rows, for its
 * centre at `center`. A row above it moves below once the centre rises above
 * that row's bottom edge; a row below it moves above once the centre passes
 * that row's top edge. So rows give way when they are half covered.
 */
export function placeLifted(rows: Row[], source: number, center: number): number {
  let at = 0;
  rows.forEach((row, i) => {
    if (i < source && center >= row.top + row.height) at++;
    else if (i > source && center > row.top) at++;
  });
  return at;
}

/** The place of a new row for the pointer at `y`: before the first row whose middle is below it. */
export function placeNew(rows: Row[], y: number): number {
  const at = rows.findIndex((row) => y < row.top + row.height / 2);
  return at < 0 ? rows.length : at;
}

export type Arrangement = {
  /** How far each row moves (positive: down), by id. */
  shift: Record<string, number>;
  /** Top of the place the moved or new row ends up in. */
  slotTop: number;
};

/**
 * The rows with a slot of `height` at place `at`: either the place a row
 * moves to (`source` is its index, left out of the others) or a new row
 * (`source` null). `start` is where the first row sits when there is none.
 */
export function arrange(
  rows: Row[],
  gap: number,
  at: number,
  source: number | null,
  height: number,
  start: number,
): Arrangement {
  const others = source === null ? rows : rows.filter((_, i) => i !== source);
  const place = Math.max(0, Math.min(at, others.length));
  const shift: Record<string, number> = {};
  let top = rows.length ? rows[0].top : start;
  let slotTop = top;
  others.forEach((row, i) => {
    if (i === place) {
      slotTop = top;
      top += height + gap;
    }
    shift[row.id] = top - row.top;
    top += row.height + gap;
  });
  if (place === others.length) slotTop = top;
  return { shift, slotTop };
}
