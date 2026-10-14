# Архитектура и последовательность PR

Предложения на 2026-10-14; окончательный выбор зависит от замеров сети, бюджета и требуемой задержки. Rust — язык сервиса; Factory-контракт в сети и паттерн Factory в приложении — разные сущности.

## Хранение и публичное API

Предлагаю PostgreSQL как постоянное хранилище: deals, snapshots, events, checkpoints. Ключ сделки включает chain ID, Factory и deal ID; адрес также уникален внутри сети. Raw JSON сохраняет доказательства и неизвестные поля, отдельные индексированные колонки поддерживают фильтры host/buyer/status/epoch. Суммы — точные decimal/строки, Uint256 требует до 78 цифр.

Кэш в памяти полезен поверх БД, но не сохраняет checkpoint после перезапуска. SQLite годится для локальной разработки; файловая система Cloud Run непостоянна, поэтому локальный файл нельзя считать production-хранилищем ([контракт Cloud Run](https://docs.cloud.google.com/run/docs/container-contract)). Объём и цена managed PostgreSQL пока не оценены.

Предлагаемые GET: `/v1/deals`, `/v1/deals/{address}`, `/v1/deals/{address}/events`, `/healthz`. Ограниченные фильтры, cursor pagination, max page size; в ответах chain ID, snapshot height/time, event checkpoint, stale и полнота истории. API читает БД, сборщик обращается в сеть независимо от пользовательских запросов.

Для защиты публичного API: Application Load Balancer + Cloud Armor с rate limiting, ingress `internal-and-cloud-load-balancing`, отключённый default run.app URL; иначе возможен обход защиты ([Cloud Armor](https://docs.cloud.google.com/armor/docs/integrating-cloud-armor), [ingress](https://docs.cloud.google.com/run/docs/securing/ingress)). В приложении — timeout, ограничения размера ответа/параметров и пула БД; лимит экземпляров сдерживает нагрузку на БД, но не заменяет DDoS-защиту. Стоимость LB/Armor учитывать отдельно.

Сборщик запускать отдельно от HTTP API: конечные пачки в Job для допускающей паузы синхронизации либо постоянно работающий worker при малой задержке. Сначала согласовать SLA; фоновый цикл в масштабируемом API создаёт риск дублирования работы. Нужны lease и безопасные повторы. Для Cloud Run hello-world слушает `0.0.0.0:$PORT` ([требования контейнера](https://docs.cloud.google.com/run/docs/container-contract)).

## Небольшие шаги реализации

Каждый PR целится в ≤50 изменённых строк; 50–100 нежелательно, >100 запрещено. Перед PR измерять добавления и удаления; Cargo.lock исключён из подсчёта по согласованию с пользователем. Остальные файлы, включая сгенерированные, учитываются. При превышении делить самостоятельные изменения.

Ниже — исходная последовательность этапов, а не список созданных PR. Каркас Rust, HTTP, тесты, инструменты, CI и контейнер уже оформлены коммитами; деплой и сборщик ещё не реализованы.

| PR | Результат | Проверка |
| --- | --- | --- |
| 1 | Git, минимальный Cargo package без зависимостей, hello-world | cargo run; отсутствие секретов в tracked files |
| 2 | Toolchain, fmt/clippy, forbid unsafe, deny warnings | fmt --check; clippy; cargo test |
| 3 | Локальный pre-commit и те же проверки CI | намеренно неверный формат блокируется |
| 4 | Маленькие модули config/domain; Factory создания адаптера | ошибка конфигурации понятна новичку |
| 5 | HTTP hello-world и health, после решения размера dependency diff | запрос к порту; graceful shutdown |
| 6 | Контейнер и затем отдельный PR конфигурации Cloud Run | локальный smoke; после деплоя health |
| 7+ | По отдельности: DTO Factory, RPC client, pagination, DTO Deal | реальные fixtures; ошибки и границы страниц |
| 8+ | Отдельные миграции, snapshots, checkpoints, event decoder | атомарность и повтор без дублей |
| 9+ | По одному API route, затем фильтры и freshness metadata | pagination; stale; неполная история |

Ориентир каталогов: `src/{config,domain,chain,storage,api}`, `src/bin` для API/collector, `tests/fixtures`, `migrations`, `deploy`, `docs`. Создавать по мере появления кода. Factory — небольшая функция сборки зависимостей; trait — контракт адаптера, Result — явный успех/ошибка, serde — преобразование JSON. Эти понятия объяснять на каждом фактическом изменении.
