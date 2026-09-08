use super::TokenPrices;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 单次已发送请求的用量；缺失值表示供应商未报告，不能视为零。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    /// 思考 Token 是输出的一部分，不重复计费。
    pub reasoning_tokens: Option<u64>,
    pub cost: Option<f64>,
    /// 保存当次价格，修改设置不重算旧账。
    pub prices: Option<TokenPrices>,
}

impl RequestUsage {
    pub(super) fn unknown(prices: Option<TokenPrices>) -> Self {
        Self {
            prices,
            ..Self::default()
        }
    }

    pub(super) fn from_response(response: &Value, prices: Option<TokenPrices>) -> Self {
        let usage = &response["usage"];
        let count = |v: &Value| v.as_u64().filter(|n| *n <= 1_000_000_000_000);
        let input = count(&usage["input_tokens"]).or_else(|| count(&usage["prompt_tokens"]));
        let output = count(&usage["output_tokens"]).or_else(|| count(&usage["completion_tokens"]));
        let cached = count(&usage["input_tokens_details"]["cached_tokens"])
            .or_else(|| count(&usage["prompt_tokens_details"]["cached_tokens"]))
            .or_else(|| count(&usage["prompt_cache_hit_tokens"]))
            .filter(|n| input.is_some_and(|total| *n <= total));
        let reasoning = count(&usage["output_tokens_details"]["reasoning_tokens"])
            .or_else(|| count(&usage["completion_tokens_details"]["reasoning_tokens"]))
            .filter(|n| output.is_some_and(|total| *n <= total));
        let cost = calculate_cost(input, output, cached, prices.as_ref());
        Self {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cached,
            reasoning_tokens: reasoning,
            cost,
            prices,
        }
    }

    pub(super) fn validate(&self) -> Result<(), &'static str> {
        if [
            self.input_tokens,
            self.output_tokens,
            self.cached_input_tokens,
            self.reasoning_tokens,
        ]
        .into_iter()
        .flatten()
        .any(|n| n > 1_000_000_000_000)
            || self
                .cached_input_tokens
                .is_some_and(|n| self.input_tokens.is_none_or(|input| n > input))
            || self
                .reasoning_tokens
                .is_some_and(|n| self.output_tokens.is_none_or(|output| n > output))
            || self.cost.is_some_and(|v| !v.is_finite() || v < 0.0)
        {
            return Err("Token 用量记录无效");
        }
        let options = super::ModelOptions {
            prices: self.prices.clone(),
            ..Default::default()
        };
        options.validate().map_err(|_| "计费价格无效")?;
        if self.cost
            != calculate_cost(
                self.input_tokens,
                self.output_tokens,
                self.cached_input_tokens,
                self.prices.as_ref(),
            )
        {
            return Err("费用与用量及价格不一致");
        }
        Ok(())
    }

    pub(super) fn total(&self) -> Option<u64> {
        self.input_tokens?.checked_add(self.output_tokens?)
    }
}

fn calculate_cost(
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
    prices: Option<&TokenPrices>,
) -> Option<f64> {
    let (input, output, p) = (input?, output?, prices?);
    let cached = if p.cached_input.is_some() { cached? } else { 0 };
    Some(
        ((input - cached) as f64 * p.input
            + cached as f64 * p.cached_input.unwrap_or(p.input)
            + output as f64 * p.output)
            / 1_000_000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prices() -> Option<TokenPrices> {
        Some(TokenPrices {
            currency: "CNY".into(),
            input: 2.0,
            output: 8.0,
            cached_input: Some(0.5),
        })
    }

    #[test]
    fn normalizes_both_protocols_without_double_counting_cache_or_reasoning() {
        for usage in [
            json!({"input_tokens":1000,"output_tokens":200,"input_tokens_details":{"cached_tokens":400},"output_tokens_details":{"reasoning_tokens":150}}),
            json!({"prompt_tokens":1000,"completion_tokens":200,"prompt_tokens_details":{"cached_tokens":400},"completion_tokens_details":{"reasoning_tokens":150}}),
            json!({"prompt_tokens":1000,"completion_tokens":200,"prompt_cache_hit_tokens":400,"completion_tokens_details":{"reasoning_tokens":150}}),
        ] {
            let record = RequestUsage::from_response(&json!({"usage":usage}), prices());
            assert_eq!(record.total(), Some(1200));
            assert_eq!(record.reasoning_tokens, Some(150));
            assert!((record.cost.unwrap() - 0.003).abs() < 1e-12);
            record.validate().unwrap();
        }
    }

    #[test]
    fn missing_usage_or_cache_discount_information_is_never_a_zero_charge() {
        let record = RequestUsage::from_response(&json!({}), prices());
        assert!(record.total().is_none() && record.cost.is_none());
        let record = RequestUsage::from_response(
            &json!({"usage":{"input_tokens":1000,"output_tokens":200}}),
            prices(),
        );
        assert_eq!(record.total(), Some(1200));
        assert!(record.cost.is_none());
        let mut price = prices().unwrap();
        price.cached_input = None;
        let record = RequestUsage::from_response(
            &json!({"usage":{"input_tokens":1000,"output_tokens":200}}),
            Some(price),
        );
        assert!((record.cost.unwrap() - 0.0036).abs() < 1e-12);
    }
}
