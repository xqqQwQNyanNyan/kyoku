import type { RequestUsage, SessionTurn } from './types';

const count = (value: number | null) => (value === null ? '未知' : value.toLocaleString());
const money = (value: number) =>
  value === 0 ? '0' : value < 0.000001 ? '<0.000001' : value.toFixed(6);

export function turnUsage(turn: SessionTurn): RequestUsage[] {
  if (turn.options || turn.usage?.length) return turn.usage ?? [];
  // 旧会话只有原始 usage；仍显示可核对的计数，不套用当前价格。
  const requests: RequestUsage[] = [];
  for (const step of turn.trace) {
    if (step.kind === 'request')
      requests.push({
        input_tokens: null,
        output_tokens: null,
        cached_input_tokens: null,
        reasoning_tokens: null,
        prices: null,
        cost: null,
      });
    if (step.kind !== 'response') continue;
    const output = step.output as { usage?: Record<string, unknown> } | undefined;
    const raw = output?.usage;
    const numeric = (value: unknown) =>
      typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null;
    const usage = {
      input_tokens: numeric(raw?.input_tokens ?? raw?.prompt_tokens),
      output_tokens: numeric(raw?.output_tokens ?? raw?.completion_tokens),
      cached_input_tokens: null,
      reasoning_tokens: null,
      prices: null,
      cost: null,
    };
    if (requests.length) requests[requests.length - 1] = usage;
    else requests.push(usage);
  }
  return requests;
}

export function UsageSummary({
  requests,
  budget,
  label = '本轮',
}: {
  requests: RequestUsage[];
  budget?: number | null;
  label?: string;
}) {
  if (!requests.length) return null;
  let input = 0,
    output = 0;
  let incomplete = false;
  let inputKnown = false,
    outputKnown = false;
  const costs = new Map<string, number>();
  for (const usage of requests) {
    inputKnown ||= usage.input_tokens !== null;
    outputKnown ||= usage.output_tokens !== null;
    input += usage.input_tokens ?? 0;
    output += usage.output_tokens ?? 0;
    incomplete ||= usage.input_tokens === null || usage.output_tokens === null;
    if (usage.cost !== null && usage.prices) {
      costs.set(usage.prices.currency, (costs.get(usage.prices.currency) ?? 0) + usage.cost);
    }
  }
  const partialCost = requests.some((u) => u.cost === null);
  return (
    <div className="usage-summary">
      <span>
        {label} · {requests.length} 次请求 · {incomplete ? '已知' : ''}输入{' '}
        {inputKnown ? count(input) : '未知'} / 输出 {outputKnown ? count(output) : '未知'} Token
      </span>
      <span>
        {costs.size
          ? `${partialCost ? '已知' : ''}估算费用 ${[...costs].map(([currency, cost]) => `${money(cost)} ${currency}`).join(' + ')}`
          : '费用未知 / 未配置价格'}
      </span>
      {budget != null && (
        <span>
          本轮预算：{incomplete ? '已知 ' : ''}
          {inputKnown || outputKnown ? count(input + output) : '未知'} / {count(budget)} Token
        </span>
      )}
      {incomplete && <span>部分请求未返回用量，实际消耗可能更高。</span>}
      {partialCost && costs.size > 0 && <span>部分费用未知，未计入小计。</span>}
    </div>
  );
}

export function UsageDetails({ requests }: { requests: RequestUsage[] }) {
  if (!requests.length) return null;
  return (
    <div className="usage-details">
      <table>
        <thead>
          <tr>
            <th>请求</th>
            <th>输入 / 输出 Token</th>
            <th>估算费用</th>
          </tr>
        </thead>
        <tbody>
          {requests.map((u, index) => (
            <tr key={index}>
              <td>{index + 1}</td>
              <td>
                {count(u.input_tokens)} / {count(u.output_tokens)}
                {u.cached_input_tokens != null && (
                  <small>缓存输入 {count(u.cached_input_tokens)}</small>
                )}
                {u.reasoning_tokens != null && <small>其中思考 {count(u.reasoning_tokens)}</small>}
              </td>
              <td>
                {u.cost !== null && u.prices
                  ? `${money(u.cost)} ${u.prices.currency}`
                  : '未知 / 未配置'}
                {u.prices && (
                  <small>
                    每百万：输入 {u.prices.input} / 输出 {u.prices.output}
                    {u.prices.cached_input != null ? ` / 缓存 ${u.prices.cached_input}` : ''}
                  </small>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
