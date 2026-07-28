# Third-party upstream providers

Token2Token can route an ordinary community node through an OpenAI-compatible upstream while keeping that upstream credential on the node.

This is transport encryption, not confidential compute. The node operator and upstream supplier remain inside the trust boundary.

| Supplier | Base URL | Credential class | Marketplace use |
| --- | --- | --- | --- |
| DeepSeek API | `https://api.deepseek.com` | Pay-as-you-go developer API key | Only under the supplier's API terms |
| Kimi Open Platform | `https://api.moonshot.cn` | Pay-as-you-go developer API key | Only under the supplier's API terms |
| Kimi Code | Supplier-specific coding endpoint | Personal subscription key | Personal use only; never publish |
| OpenAI API | `https://api.openai.com` | Developer API key | Only under the supplier's API terms |
| ChatGPT/Codex login | Official Codex clients | Consumer OAuth | Never publish or resell |

The client requires the operator to confirm commercial-hosting rights before any discovered model is published. Token2Token may quarantine a node or model immediately if a supplier changes its rules or reports misuse.
