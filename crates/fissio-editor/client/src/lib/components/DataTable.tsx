import { For, Show, JSX } from 'solid-js';

export interface Column<T> {
  key: string;
  header: string;
  align?: 'left' | 'center' | 'right';
  width?: string;
  render?: (row: T) => JSX.Element | string | number;
}

interface Props<T> {
  columns: Column<T>[];
  data: T[];
  loading?: boolean;
  emptyMessage?: string;
  onRowClick?: (row: T) => void;
  selectedRow?: T | null;
  rowKey: (row: T) => string;
}

export default function DataTable<T>(props: Props<T>) {
  const getCellValue = (row: T, col: Column<T>): JSX.Element | string | number => {
    if (col.render) return col.render(row);
    return (row as Record<string, unknown>)[col.key] as string | number;
  };

  const alignClass = (align?: 'left' | 'center' | 'right'): string => {
    const map = { left: 'text-left', center: 'text-center', right: 'text-right' };
    return map[align || 'left'];
  };

  return (
    <div class="data-table-container">
      <table class="data-table">
        <thead>
          <tr>
            <For each={props.columns}>
              {(col) => (
                <th class={alignClass(col.align)} style={{ width: col.width }}>
                  {col.header}
                </th>
              )}
            </For>
          </tr>
        </thead>
        <tbody>
          <Show when={!props.loading} fallback={
            <tr><td colspan={props.columns.length} class="empty-cell">Loading...</td></tr>
          }>
            <Show when={props.data.length > 0} fallback={
              <tr><td colspan={props.columns.length} class="empty-cell">{props.emptyMessage || 'No data'}</td></tr>
            }>
              <For each={props.data}>
                {(row) => (
                  <tr
                    class={props.onRowClick ? 'clickable' : ''}
                    classList={{ selected: props.selectedRow && props.rowKey(props.selectedRow) === props.rowKey(row) }}
                    onClick={() => props.onRowClick?.(row)}
                  >
                    <For each={props.columns}>
                      {(col) => (
                        <td class={alignClass(col.align)}>
                          {getCellValue(row, col)}
                        </td>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            </Show>
          </Show>
        </tbody>
      </table>
    </div>
  );
}
