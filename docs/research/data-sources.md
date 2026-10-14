# Данные мониторинга Gonka Forward

Исследование 2026-10-14. Проверены исходники [forward-contracts](https://github.com/gonka24/forward-contracts/tree/4092b665c97b87f937d4c4c230870cebb038a4d8), commit `4092b665c97b87f937d4c4c230870cebb038a4d8`. Это проверка интерфейсов, не выгрузка из сети. В рабочем каталоге до исследования был только `init.local.md`, Git ещё не инициализирован.

## Что можно получить

| Потребность | Запрос | Данные и ограничения |
| --- | --- | --- |
| Все сделки | Factory: `{"list_deals":{"start_after":null,"limit":50}}` | `deals:[{id,address}]`; ID по возрастанию, следующий cursor — последний ID; default 20, max 50, cursor исключающий |
| Проверка регистрации | Factory: `{"deal":{"id":1}}` или `deal_by_host_epoch` | Адрес сделки; пара Host/epoch сохраняется после завершения |
| Участники и условия | Deal: `{"config":{}}`, `{"state":{}}` | Host, Factory, fee recipient, CW20, epoch, цена, бюджет; Buyer в state может быть null |
| Текущее состояние | Deal: `{"state":{}}` | `open`, `funded`, `locked`, `releasing`, `completed`, `refunded`, `cancelled`, `expired`; причина refund и учёт GNK/USDT |
| Факт финансирования | Deal: `{"funding":{}}` | `funded`, Buyer, бюджет и capacity; сами условия сделки не доказывают оплату |
| Долги и выплаты | Deal: `{"usdt_payments":{}}` | По ролям host/fee/buyer: recipient, accrued, paid, pending; одинаковые адреса не объединяют роли |
| Права и выдача GNK | Deal: `entitlements`, `release_status` с пустым объектом | Доли и накопленные выплаты; lifetime counters имеют тип Uint256 |
| Баланс и vesting | Deal: `{"native_status":{}}` | liquid/remaining vesting; ошибка диагностики не означает нулевой баланс |

Проверено по [Factory API](https://github.com/gonka24/forward-contracts/blob/4092b665c97b87f937d4c4c230870cebb038a4d8/packages/marketplace-api/src/factory.rs) и [Deal API](https://github.com/gonka24/forward-contracts/blob/4092b665c97b87f937d4c4c230870cebb038a4d8/packages/marketplace-api/src/deal.rs). Uint128/Uint256 передаются десятичными строками; хранить без float. ID/epoch — u64: в нашем публичном API безопаснее строки для клиентов JavaScript.

## События и смысл состояния

Factory выпускает `offer_created` с deal_id, deal, host, target_epoch, ценой и бюджетом. Deal выпускает `deal_funded`, `deal_locked`, `deal_cancelled`, `claim_settled`, `usdt_paid`, `usdt_refunded`, `gnk_released`, `deal_completed`, `deal_refunded`, `deal_expired`, `excess_gnk_forwarded`. В сети возможен префикс `wasm-`; фактический формат требуется записать из успешной транзакции. Источники: [Factory reply](https://github.com/gonka24/forward-contracts/blob/4092b665c97b87f937d4c4c230870cebb038a4d8/contracts/marketplace-factory/src/reply.rs), [Deal implementation](https://github.com/gonka24/forward-contracts/blob/4092b665c97b87f937d4c4c230870cebb038a4d8/contracts/marketplace-deal/src/contract.rs).

`completed` не доказывает погашение USDT или окончание vesting. Поздние GNK могут распределяться и после него. Native unlock и прямой перевод не обязаны вызывать Deal; одних событий контракта недостаточно. Нужны повторные запросы, включая terminal deals. `network_unconfirmed` не доказывает отсутствие claim. Эти правила описаны в [архитектуре](https://github.com/gonka24/forward-contracts/blob/4092b665c97b87f937d4c4c230870cebb038a4d8/docs/architecture.md).

## Что пока неизвестно

В проверенном дереве нет подтверждённого deployment receipt: в `release/examples` адреса и chain ID — placeholders. Нужны целевая сеть, Factory, высота создания, RPC и REST/gRPC, code IDs/checksums. Наличие кода не доказывает совместимость развёрнутого runtime. См. [требования сети](https://github.com/gonka24/forward-contracts/blob/4092b665c97b87f937d4c4c230870cebb038a4d8/docs/chain-requirements.md).

Следующие документы: [проверка и сбор](collection-plan.md), [архитектура и маленькие PR](implementation-plan.md).
