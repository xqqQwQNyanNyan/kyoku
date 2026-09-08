use super::AgentError;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

/// 仅在用户明确选择时发送；具体取值是否受支持由所选模型决定。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Thinking {
    #[default]
    Default,
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

/// 兼容服务使用的 Chat 输出上限参数；Responses 始终使用 max_output_tokens。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatTokenLimit {
    #[default]
    MaxCompletionTokens,
    MaxTokens,
}

/// 用户填写的每百万 Token 价格；不内置可能过时的供应商报价。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPrices {
    pub currency: String,
    pub input: f64,
    pub output: f64,
    /// 留空时缓存输入按普通输入价格估算。
    pub cached_input: Option<f64>,
}

/// 模型生成、上下文和单轮预算配置；请求前统一校验。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelOptions {
    pub max_output_tokens: NonZeroU64,
    /// 输入和输出共用的上下文窗口；本地按保守字节估算检查，不裁剪历史。
    pub context_tokens: Option<NonZeroU64>,
    pub thinking: Thinking,
    pub chat_token_limit: ChatTokenLimit,
    /// 每轮提问的输入加输出额度；追问和显式重试重新计数。
    pub token_budget: Option<NonZeroU64>,
    pub prices: Option<TokenPrices>,
}

impl Default for ModelOptions {
    fn default() -> Self {
        Self {
            max_output_tokens: NonZeroU64::new(4096).unwrap(),
            context_tokens: None,
            thinking: Thinking::Default,
            chat_token_limit: ChatTokenLimit::default(),
            token_budget: None,
            prices: None,
        }
    }
}

impl ModelOptions {
    /// 校验数值和币种；不联网，也不猜测模型的能力。
    pub fn validate(&self) -> Result<(), AgentError> {
        let bad = |field, reason| AgentError::InvalidConfig { field, reason };
        for (field, value) in [
            ("max_output_tokens", Some(self.max_output_tokens)),
            ("context_tokens", self.context_tokens),
            ("token_budget", self.token_budget),
        ] {
            if value.is_some_and(|v| v.get() > 1_000_000_000) {
                return Err(bad(field, "Token 上限不能超过 10 亿"));
            }
        }
        if self
            .context_tokens
            .is_some_and(|v| v <= self.max_output_tokens)
        {
            return Err(bad("context_tokens", "上下文长度必须大于输出上限"));
        }
        if let Some(prices) = &self.prices {
            if prices.currency.len() != 3
                || !prices.currency.bytes().all(|c| c.is_ascii_uppercase())
            {
                return Err(bad(
                    "currency",
                    "请填写三个大写字母的币种代码，如 CNY 或 USD",
                ));
            }
            if [Some(prices.input), Some(prices.output), prices.cached_input]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0.0..=1_000_000_000.0).contains(&v))
            {
                return Err(bad("prices", "价格必须是 0 到 10 亿之间的有限数值"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_partial_configuration_preserve_model_defaults() {
        let options: ModelOptions =
            serde_json::from_value(json!({"max_output_tokens": 16384})).unwrap();
        options.validate().unwrap();
        assert_eq!(options.max_output_tokens.get(), 16384);
        assert_eq!(options.thinking, Thinking::Default);
        assert!(options.token_budget.is_none());
    }

    #[test]
    fn invalid_limits_and_prices_are_rejected() {
        for value in [
            json!({"max_output_tokens":0}),
            json!({"token_budget":-1}),
            json!({"context_tokens":1.5}),
            json!({"thinking":"unknown"}),
            json!({"typo":5}),
        ] {
            assert!(serde_json::from_value::<ModelOptions>(value).is_err());
        }
        for value in [
            json!({"context_tokens":4096}),
            json!({"max_output_tokens":1000000001}),
            json!({"prices":{"currency":"usd","input":1,"output":1}}),
            json!({"prices":{"currency":"USD","input":-1,"output":1}}),
        ] {
            assert!(
                serde_json::from_value::<ModelOptions>(value)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }
}
