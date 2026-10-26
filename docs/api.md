# Возможности и HTTP API

Сервис читает сделки pilot Factory `gonka1ea2s57dvs06fr5mjzmna9w9vt0xylrna5cjtmzr5h72vkrza3xxsp7eayy`. Адрес Factory закреплён в коде. Произвольные контракты вне её реестра не поддерживаются.
API только читает данные: создание сделок, транзакции и управление контрактами отсутствуют.
В приложении нет авторизации; доступ к развёрнутому сервису может ограничивать IAM Cloud Run. Все маршруты `/v1/deals` читают память, не вызывая ноду при HTTP-запросе.

## Быстрая проверка

Запустите `cargo run` из корня репозитория, затем в другом терминале:

```sh
base_url=http://localhost:8080
address=gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs
curl -i "$base_url/"
curl -i "$base_url/healthz"
curl -i "$base_url/v1/deals"
curl -i "$base_url/v1/deals?after=1"
curl -i "$base_url/v1/deals/$address"
curl -i "$base_url/v1/deals/$address/events"
```

Для Cloud Run замените `base_url` на URL из Actions Summary. До готовности кэша ожидается 503. `GET /` возвращает текст `Hello, world!\n`; `GET /healthz` — `ok\n`, оба с кодом 200.
`/healthz` проверяет процесс, но не доступность ноды, заполнение или свежесть кэша.

## Общие поля и актуальность v1

| Поле | Тип | Значение |
|---|---|---|
| `updated_at` | integer | Unix seconds начала сбора снимка, не время события или блока |
| `stale` | boolean | Последний сбор не удался либо возраст снимка достиг 120 секунд |

Сбор начинается при старте и повторяется раз в 60 секунд; общий таймаут — 50 секунд.
Состояния и истории заменяются вместе только после успешного сбора всех сделок. При сбое остаётся предыдущий снимок с `stale=true` и HTTP 200; до первого снимка — 503.
Кэш живёт в памяти каждого процесса, теряется при перезапуске и не разделяется между экземплярами.
Снимок не атомарен по высоте блока. `stale=false` не гарантирует актуальность самой ноды.

## POST /v2/deals/search

Прямой поиск событий в сети по hosts/buyers, проверка Factory и чтение деталей
найденных сделок. Cursor продолжает события; снимка состояния нет.
[Контракт и ограничения](search.md),
[OpenAPI](openapi-search.json). Старые маршруты v1 сохраняют описанный ниже контракт.

## GET /v1/deals

Возвращает до 50 пар `id`/`address` по возрастанию ID. Размер страницы не настраивается.
Необязательный `after` — десятичный u64 (0…18446744073709551615), исключительная нижняя граница ID.
Без него выдаётся первая страница. Для следующей передайте `next_after` как `after`.
`next_after=null` означает конец; после последнего ID возвращаются `deals: []` и `next_after: null`. Курсор не закрепляет снимок: между запросами данные могут обновиться. ID и курсор в JSON — строки. Примеры ниже иллюстративные, не отражают текущее состояние сети.

```json
{"deals":[{"id":"1","address":"gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs"}],"next_after":null,"updated_at":1792000000,"stale":false}
```

## GET /v1/deals/{address}

Адрес должен точно совпадать с адресом сделки в кэше. Ответ — объект сделки:

```json
{"id":"1","address":"gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs","status":"releasing","buyer":"gonka1ww2p5wv8plwte5tr00spkv0vt2hzr9s9kppd0x","host":"gonka1hvk3t5c4hzn0q8q3lvg6l7k4dfvfa5xqj4a66m","target_epoch":"423","factory":"gonka1ea2s57dvs06fr5mjzmna9w9vt0xylrna5cjtmzr5h72vkrza3xxsp7eayy","buyer_budget_micro_usdt":"140000000","price_micro_usdt_per_gnk":"200000","settlement_cw20":"gonka15ggwj9un6qrmu4nj5ev6l7kpdcr00td03ff2mmj4cyhl8u8vjd2qnl3hgk","updated_at":1792000000,"stale":false}
```

| Поле | Тип | Значение |
|---|---|---|
| `id`, `address`, `factory` | string | ID в реестре, адрес сделки и адрес Factory |
| `buyer` | string или null | Покупатель, если указан в состоянии контракта |
| `host` | string | Адрес хоста из конфигурации |
| `status` | string | Статус контракта, список ниже |
| `target_epoch` | string | Номер целевой эпохи |
| `buyer_budget_micro_usdt` | string | Бюджет покупателя в микро-USDT |
| `price_micro_usdt_per_gnk` | string | Цена в микро-USDT за GNK |
| `settlement_cw20` | string | Адрес расчётного CW20-токена |

Поддерживаемые статусы: `open`, `funded`, `locked`, `releasing`, `completed`, `refunded`, `cancelled`, `expired`.
API передаёт статус контракта, не вычисляет его из событий. Неизвестный статус прерывает сбор. Числовые строки сохраняют точность: не преобразуйте суммы в floating point. 1 USDT = 1 000 000 микро-USDT.
Фактические выплаты, комиссии и остатки из state не включены в этот ответ; бюджет не является балансом.

## GET /v1/deals/{address}/events

Возвращает `address`, массив `events`, `updated_at`, `stale`. Известная сделка без событий даёт 200 и `events: []`.
Пагинации и фильтров HTTP нет: возвращается вся собранная история сделки.
Пример структуры с сокращёнными атрибутами события:

```json
{"address":"gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs","events":[{"transaction_hash":"76A75CDE1C93E3C32B927F340365F30B8D485A678F95A5665D87C2265BCC16CD","height":"6519973","event_index":11,"event":{"type":"wasm-deal_funded","attributes":[{"key":"buyer_budget_micro_usdt","value":"140000000"}]}}],"updated_at":1792000000,"stale":false}
```

`transaction_hash` и `height` — строки; `event_index` — integer, исходная позиция события в транзакции с нуля.
`event.type` — строка; `event.attributes` — массив строковых пар `key`/`value`, сохраняющий порядок и повторяющиеся ключи.
История выбирается через RPC `tx_search` по `execute._contract_address`, в порядке `asc`, внутри транзакции — исходный порядок. В ответ входят только успешные транзакции (`code=0`) и события с единственным совпадающим `_contract_address`.
Сохраняются также служебные `execute`, а не только `wasm-*`. Одинаковые события на разных индексах не склеиваются. Идентификатор записи — пара хеш/индекс. Индекс ноды не гарантирует полноту истории; доказательства и реорганизации не проверяются.

## Ошибки и лимиты

| HTTP | Когда |
|---|---|
| 400 | Неверный `after`: пустая строка, не число, отрицательное значение или переполнение u64 |
| 404 | Адрес не найден в готовом кэше (включая неверный формат); неизвестный маршрут |
| 503 | Кэш ещё не опубликован или недоступен; у адресных маршрутов проверяется раньше поиска адреса |

Ошибки 404/503 обработчиков не имеют JSON-тела; 400 формирует Axum. Общей JSON-схемы ошибок нет.
Лимиты сбора: 1000 сделок; на сделку — 1000 транзакций и 10 000 подходящих событий; запрос к ноде — до 10 секунд. История сделки ограничена 50 секундами и общим 50-секундным бюджетом обновления. Превышение отменяет весь новый снимок.
Повтор хеша, изменение `total_count`, неполная страница истории или ошибка RPC также отменяют сбор.
Нет БД, подписок/WebSocket, алертов, OpenAPI/Swagger и встроенного rate limiting. Деплой: [deploy.md](deploy.md).

## Live routing (v0.2.0)

`GET /v1/deals/{deal}/routing` returns a fresh, fully verified routing observation
with four explicit states and `Cache-Control: no-store`. It is independent of
cached deal state. See [routing contract, errors and integration](routing.md).
