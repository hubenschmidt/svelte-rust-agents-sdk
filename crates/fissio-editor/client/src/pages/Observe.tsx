import { createSignal, createResource, For, Show } from 'solid-js';
import { A } from '@solidjs/router';
import DataTable, { Column } from '../lib/components/DataTable';

const API_BASE = 'http://localhost:8000';

interface TraceRecord {
  trace_id: string;
  pipeline_id: string;
  pipeline_name: string;
  timestamp: number;
  input: string;
  output: string;
  total_elapsed_ms: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_tool_calls: number;
  status: string;
}

interface SpanRecord {
  span_id: string;
  trace_id: string;
  node_id: string;
  node_type: string;
  start_time: number;
  end_time: number;
  input: string;
  output: string;
  input_tokens: number;
  output_tokens: number;
  tool_call_count: number;
  iteration_count: number;
}

interface MetricsSummary {
  total_traces: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_tool_calls: number;
  avg_latency_ms: number;
}

const fetchTraces = async (): Promise<TraceRecord[]> => {
  const res = await fetch(`${API_BASE}/api/traces?limit=50`);
  const data = await res.json();
  return data.traces;
};

const fetchMetrics = async (): Promise<MetricsSummary> => {
  const res = await fetch(`${API_BASE}/api/metrics/summary`);
  return res.json();
};

const fetchTraceDetail = async (traceId: string): Promise<{ trace: TraceRecord; spans: SpanRecord[] }> => {
  const res = await fetch(`${API_BASE}/api/traces/${traceId}`);
  return res.json();
};

const formatTimestamp = (ts: number): string => {
  const d = new Date(ts);
  return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

const formatDuration = (ms: number): string => {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
};

const truncate = (str: string, len: number): string => {
  return str.length > len ? str.slice(0, len) + '...' : str;
};

export default function Observe() {
  const [traces, { refetch: refetchTraces }] = createResource(fetchTraces);
  const [metrics] = createResource(fetchMetrics);
  const [selectedTrace, setSelectedTrace] = createSignal<TraceRecord | null>(null);
  const [traceDetail, setTraceDetail] = createSignal<{ trace: TraceRecord; spans: SpanRecord[] } | null>(null);

  const selectTrace = async (trace: TraceRecord) => {
    setSelectedTrace(trace);
    const detail = await fetchTraceDetail(trace.trace_id);
    setTraceDetail(detail);
  };

  const closeDetail = () => {
    setSelectedTrace(null);
    setTraceDetail(null);
  };

  const columns: Column<TraceRecord>[] = [
    {
      key: 'timestamp',
      header: 'Time',
      width: '160px',
      render: (row) => formatTimestamp(row.timestamp)
    },
    {
      key: 'pipeline_name',
      header: 'Pipeline',
      render: (row) => truncate(row.pipeline_name, 30)
    },
    {
      key: 'input',
      header: 'Input',
      render: (row) => truncate(row.input, 35)
    },
    {
      key: 'tokens',
      header: 'Tokens',
      align: 'right',
      width: '80px',
      render: (row) => (row.total_input_tokens + row.total_output_tokens).toLocaleString()
    },
    {
      key: 'latency',
      header: 'Latency',
      align: 'right',
      width: '80px',
      render: (row) => formatDuration(row.total_elapsed_ms)
    },
    {
      key: 'status',
      header: 'Status',
      align: 'center',
      width: '90px',
      render: (row) => <span class={`status-badge ${row.status}`}>{row.status}</span>
    }
  ];

  return (
    <div class="observe-page">
      <header class="observe-header">
        <div class="observe-header-left">
          <A href="/" class="observe-back">&larr; Back</A>
          <h1>Observability</h1>
        </div>
        <button class="btn-secondary" onClick={() => refetchTraces()}>
          Refresh
        </button>
      </header>

      <div class="observe-content">
        <Show when={metrics()}>
          {(m) => (
            <div class="metrics-grid">
              <div class="metric-card">
                <div class="metric-card-label">Total Traces</div>
                <div class="metric-card-value">{m().total_traces}</div>
              </div>
              <div class="metric-card">
                <div class="metric-card-label">Input Tokens</div>
                <div class="metric-card-value">{m().total_input_tokens.toLocaleString()}</div>
              </div>
              <div class="metric-card">
                <div class="metric-card-label">Output Tokens</div>
                <div class="metric-card-value">{m().total_output_tokens.toLocaleString()}</div>
              </div>
              <div class="metric-card">
                <div class="metric-card-label">Tool Calls</div>
                <div class="metric-card-value">{m().total_tool_calls}</div>
              </div>
              <div class="metric-card">
                <div class="metric-card-label">Avg Latency</div>
                <div class="metric-card-value">{formatDuration(m().avg_latency_ms)}</div>
              </div>
            </div>
          )}
        </Show>

        <div class="observe-body">
          <div class="traces-section">
            <h2>Recent Traces</h2>
            <DataTable
              columns={columns}
              data={traces() || []}
              loading={traces.loading}
              emptyMessage="No traces recorded yet"
              onRowClick={selectTrace}
              selectedRow={selectedTrace()}
              rowKey={(row) => row.trace_id}
            />
          </div>

          <Show when={traceDetail()}>
            {(detail) => (
              <div class="trace-detail-panel">
                <div class="trace-detail-header">
                  <h3>Trace Detail</h3>
                  <button class="trace-detail-close" onClick={closeDetail}>&times;</button>
                </div>
                <div class="trace-detail-content">
                  <div class="trace-info-grid">
                    <span class="trace-info-label">Pipeline</span>
                    <span>{detail().trace.pipeline_name}</span>
                    <span class="trace-info-label">Status</span>
                    <span class={`status-badge ${detail().trace.status}`}>{detail().trace.status}</span>
                    <span class="trace-info-label">Tokens</span>
                    <span>{(detail().trace.total_input_tokens + detail().trace.total_output_tokens).toLocaleString()}</span>
                    <span class="trace-info-label">Latency</span>
                    <span>{formatDuration(detail().trace.total_elapsed_ms)}</span>
                  </div>

                  <div class="trace-io-section">
                    <h4>Input</h4>
                    <div class="trace-io-content">{detail().trace.input}</div>
                  </div>

                  <div class="trace-io-section">
                    <h4>Output</h4>
                    <div class="trace-io-content">{truncate(detail().trace.output, 500)}</div>
                  </div>

                  <Show when={detail().spans.length > 0}>
                    <div class="trace-spans-section">
                      <h4>Execution Timeline</h4>
                      <For each={detail().spans}>
                        {(span) => (
                          <div class="span-card">
                            <div class="span-header">
                              <span class="span-node-id">{span.node_id}</span>
                              <span class="span-node-type">{span.node_type}</span>
                            </div>
                            <div class="span-metrics">
                              <span>Tokens: {span.input_tokens + span.output_tokens}</span>
                              <span>Time: {formatDuration(span.end_time - span.start_time)}</span>
                              <span>Tools: {span.tool_call_count}</span>
                            </div>
                            <Show when={span.input}>
                              <div class="span-io">In: {truncate(span.input, 80)}</div>
                            </Show>
                            <Show when={span.output}>
                              <div class="span-io">Out: {truncate(span.output, 80)}</div>
                            </Show>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
}
