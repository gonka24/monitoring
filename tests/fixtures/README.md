# Deal state fixture

`deal-state.json` — неизменённое JSON-содержимое реального REST-ответа.
Источник: https://node1.gonka.ai:8443/chain-api/cosmwasm/wasm/v1/contract/gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs/smart/eyJzdGF0ZSI6e319
Получено 2026-10-14 17:31:04 UTC, `x-cosmos-block-height: 6554676`, сеть `gonka-mainnet`.
Модель пока читает только status/buyer; остальные поля ответа игнорируются.
Второй тест использует синтетические значения для проверки ошибок и отсутствия покупателя.

`factory-deals.json` — реальный list_deals (limit=50, start_after=null),
Factory `gonka1ea2s57dvs06fr5mjzmna9w9vt0xylrna5cjtmzr5h72vkrza3xxsp7eayy`,
тот же REST endpoint, высота 6554679, 2026-10-14 17:31 UTC.
