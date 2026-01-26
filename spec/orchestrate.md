# /orchestrate Mode Implementation

## Goal
Add a new `/orchestrate` command that guides the LLM to design hierarchical agent organizations - an orchestrator with sub-agents arranged like an org chart with different levels, roles, and responsibilities.

## Approach
Refactor existing compose mode into a unified "design mode" system that supports multiple design types. This is cleaner and extensible for future modes.

## Key Differences: /compose vs /orchestrate
| Aspect | /compose | /orchestrate |
|--------|----------|---------------|
| Focus | General pipeline patterns | Hierarchical org structures |
| Node emphasis | All 9 types equally | Orchestrator, coordinator, worker hierarchy |
| Edge emphasis | All 4 types | Dynamic (orchestrator→workers), direct (reporting chains) |
| Mental model | Data flow pipeline | Organizational chart |

## Files to Modify

### 1. `client/src/lib/stores/chat.ts`

**Refactor state:**
```typescript
// Before (compose-specific)
const composeMode = writable<'idle' | 'composing' | 'finalizing'>('idle');
const composeDraft = writable<Partial<PipelineInfo> | null>(null);

// After (unified design mode)
type DesignType = 'compose' | 'orchestrate';
type DesignState = 'idle' | 'designing' | 'finalizing';
const designMode = writable<{ type: DesignType | null; state: DesignState }>({ type: null, state: 'idle' });
const designDraft = writable<Partial<PipelineInfo> | null>(null);
```

**Add prompts map:**
```typescript
const DESIGN_PROMPTS: Record<DesignType, string> = {
  compose: COMPOSE_SYSTEM_PROMPT,
  orchestrate: ORCHESTRATE_SYSTEM_PROMPT
};
```

**Update functions:**
- `enterComposeMode()` → `enterDesignMode('compose')`
- `exitComposeMode()` → `exitDesignMode()`
- Add `enterDesignMode(type: DesignType)` unified function
- Update `send()` to check `designMode.state === 'designing'`
- Update `handleStreamEnd()` to check design mode

### 2. `client/src/routes/+page.svelte`

**Command handling:**
```typescript
if (trimmed === '/compose') {
  chat.enterDesignMode('compose');
  // ...
}
if (trimmed === '/orchestrate') {
  chat.enterDesignMode('orchestrate');
  // ...
}
```

**UI updates:**
- Change `$composeMode` checks to `$designMode.state`
- Show mode-specific badge text based on `$designMode.type`

## ORCHESTRATE_SYSTEM_PROMPT (Draft)

```
You are an organizational design assistant helping users create hierarchical agent teams.

**Organizational Roles:**
- orchestrator: Top-level decision maker that decomposes tasks and dispatches to workers
- coordinator: Mid-level manager that oversees a group of workers
- worker: Executes specific tasks assigned by orchestrator/coordinator
- synthesizer: Combines outputs from multiple team members
- evaluator: Reviews work quality, can request revisions

**Reporting Structures:**
- direct: Standard reporting line (worker reports to manager)
- dynamic: Orchestrator assigns tasks to workers at runtime
- feedback: Evaluator sends work back for revision

**Design Approach:**
1. Understand the user's domain and team requirements
2. Suggest appropriate hierarchy depth (flat vs multi-level)
3. Define clear roles and responsibilities for each agent
4. Establish reporting chains and communication patterns
5. Consider workload distribution and specialization

When user says "/done", output the org structure as JSON:
\`\`\`json
{
  "name": "Team Name",
  "description": "What this team does",
  "nodes": [
    { "id": "ceo", "node_type": "orchestrator", "prompt": "You are the CEO..." },
    { "id": "manager_1", "node_type": "coordinator", "prompt": "You manage..." },
    { "id": "worker_1", "node_type": "worker", "prompt": "You handle..." }
  ],
  "edges": [
    { "from": "input", "to": "ceo" },
    { "from": "ceo", "to": "manager_1", "edge_type": "dynamic" },
    { "from": "manager_1", "to": "worker_1", "edge_type": "direct" },
    { "from": "worker_1", "to": "output" }
  ]
}
\`\`\`
```

## Implementation Steps

### Step 1: Refactor chat.ts state
- Replace `composeMode` with `designMode: { type, state }`
- Replace `composeDraft` with `designDraft`
- Add `ORCHESTRATE_SYSTEM_PROMPT` constant
- Add `DESIGN_PROMPTS` map

### Step 2: Refactor chat.ts functions
- Create `enterDesignMode(type: DesignType)` - replaces `enterComposeMode()`
- Create `exitDesignMode()` - replaces `exitComposeMode()`
- Update `send()` to use `designMode.state === 'designing'`
- Update `handleStreamEnd()` to check `designMode.state`

### Step 3: Update +page.svelte
- Import new stores (`designMode`, `designDraft`)
- Handle `/orchestrate` command alongside `/compose`
- Handle `/done` for any active design mode
- Update UI conditionals from `$composeMode` to `$designMode.state`
- Show mode-specific badge: "COMPOSE MODE" vs "ORCHESTRATE MODE"

### Step 4: Update exports
- Export new stores and functions from chat store return object

## Verification
1. `/compose` still works as before (regression test)
2. `/orchestrate` shows "ORCHESTRATE MODE" indicator
3. Describe a team structure - LLM asks clarifying questions about hierarchy
4. `/done` outputs JSON, preview panel appears
5. "Save & Use" saves pipeline, can view in /composer
6. Org chart renders correctly with hierarchical layout
