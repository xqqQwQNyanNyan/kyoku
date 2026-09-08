import { Select } from './Select';
import type { ModelOptions, TokenPrices } from './types';

export const defaultModelOptions: ModelOptions = {
  max_output_tokens: 4096,
  context_tokens: null,
  thinking: 'default',
  chat_token_limit: 'max_completion_tokens',
  token_budget: null,
  prices: null,
};

export function ModelSettings({
  value,
  onChange,
  disabled = false,
}: {
  disabled?: boolean;
  value: ModelOptions;
  onChange: (value: ModelOptions) => void;
}) {
  const change = (patch: Partial<ModelOptions>) => onChange({ ...value, ...patch });
  return (
    <>
      <div className="settings-credentials">
        <div>
          <label htmlFor="llm-thinking">思考模式</label>
          <Select
            id="llm-thinking"
            label="思考模式"
            value={value.thinking}
            disabled={disabled}
            options={[
              { value: 'default', label: '模型默认（不发送参数）' },
              { value: 'none', label: '关闭（none）' },
              ...['minimal', 'low', 'medium', 'high', 'xhigh', 'max'].map((value) => ({
                value,
                label: value,
              })),
            ]}
            onChange={(thinking) => change({ thinking: thinking as ModelOptions['thinking'] })}
          />
        </div>
        <div>
          <label htmlFor="llm-output-field">Chat 输出参数</label>
          <Select
            id="llm-output-field"
            label="Chat 输出参数"
            value={value.chat_token_limit}
            disabled={disabled}
            options={[
              { value: 'max_completion_tokens', label: 'max_completion_tokens' },
              { value: 'max_tokens', label: 'max_tokens', description: '兼容旧接口' },
            ]}
            onChange={(chat_token_limit) =>
              change({ chat_token_limit: chat_token_limit as ModelOptions['chat_token_limit'] })
            }
          />
        </div>
      </div>
      <small>
        不支持思考参数时选“模型默认”。Chat 输出参数按服务商要求选择，DeepSeek 官方自动使用
        max_tokens。
      </small>
      <div className="settings-credentials">
        <div>
          <label htmlFor="llm-output">单次输出上限（Token）</label>
          <input
            id="llm-output"
            type="number"
            min="1"
            max="1000000000"
            step="1"
            required
            value={value.max_output_tokens}
            onChange={(e) => change({ max_output_tokens: Number(e.target.value) })}
          />
        </div>
        <div>
          <label htmlFor="llm-context">上下文长度（Token，可留空）</label>
          <input
            id="llm-context"
            type="number"
            min={value.max_output_tokens + 1}
            max="1000000000"
            step="1"
            placeholder="不设本地限制"
            value={value.context_tokens ?? ''}
            onChange={(e) =>
              change({ context_tokens: e.target.value === '' ? null : Number(e.target.value) })
            }
          />
        </div>
      </div>
      <small>
        上下文包含输入和输出，预检采用保守估算，不会裁剪历史。实际 Token 以服务商返回为准。
      </small>
    </>
  );
}

export function UsageSettings({
  value,
  onChange,
}: {
  value: ModelOptions;
  onChange: (value: ModelOptions) => void;
}) {
  const change = (patch: Partial<ModelOptions>) => onChange({ ...value, ...patch });
  const price = (patch: Partial<TokenPrices>) => {
    if (value.prices) change({ prices: { ...value.prices, ...patch } });
  };
  return (
    <>
      <label htmlFor="llm-budget">每轮 Token 预算（可留空）</label>
      <input
        id="llm-budget"
        type="number"
        min="1"
        max="1000000000"
        step="1"
        placeholder="不限制"
        value={value.token_budget ?? ''}
        onChange={(e) =>
          change({ token_budget: e.target.value === '' ? null : Number(e.target.value) })
        }
      />
      <small>
        累计本轮输入与输出，预算不足或用量未知时停止继续调用。追问和重试重新计数；本地预算不是账单硬限额。
      </small>
      <label className="settings-clear">
        <input
          type="checkbox"
          checked={value.prices !== null}
          onChange={(e) =>
            change({
              prices: e.target.checked
                ? { currency: 'CNY', input: 0, output: 0, cached_input: null }
                : null,
            })
          }
        />
        配置价格，估算 API 费用
      </label>
      {value.prices && (
        <>
          <div className="settings-credentials">
            <div>
              <label htmlFor="llm-currency">币种代码</label>
              <input
                id="llm-currency"
                required
                pattern="[A-Z]{3}"
                maxLength={3}
                value={value.prices.currency}
                onChange={(e) => price({ currency: e.target.value.toUpperCase() })}
              />
            </div>
            <div>
              <label htmlFor="llm-cache-price">缓存输入价格 / 百万 Token（可留空）</label>
              <input
                id="llm-cache-price"
                type="number"
                min="0"
                max="1000000000"
                step="any"
                placeholder="按普通输入价格估算"
                value={value.prices.cached_input ?? ''}
                onChange={(e) =>
                  price({ cached_input: e.target.value === '' ? null : Number(e.target.value) })
                }
              />
            </div>
          </div>
          <div className="settings-credentials">
            <div>
              <label htmlFor="llm-input-price">输入价格 / 百万 Token</label>
              <input
                id="llm-input-price"
                type="number"
                min="0"
                max="1000000000"
                step="any"
                required
                value={value.prices.input}
                onChange={(e) => price({ input: Number(e.target.value) })}
              />
            </div>
            <div>
              <label htmlFor="llm-output-price">输出价格 / 百万 Token</label>
              <input
                id="llm-output-price"
                type="number"
                min="0"
                max="1000000000"
                step="any"
                required
                value={value.prices.output}
                onChange={(e) => price({ output: Number(e.target.value) })}
              />
            </div>
          </div>
          <small>
            请填写服务商当前价格；费用仅供估算，以账单为准。思考 Token 已包含在输出中，不重复计费。
          </small>
        </>
      )}
    </>
  );
}
