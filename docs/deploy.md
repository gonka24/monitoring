# Cloud Run

Сервис слушает `0.0.0.0:$PORT`, по умолчанию 8080, обрабатывает SIGTERM.
Контейнер запускается от непривилегированного пользователя.
Контракт: https://docs.cloud.google.com/run/docs/container-contract

## GitHub Actions

Workflow `Rust` проверяет код в pull request и при push в `main`, без двойных запусков на ветках PR. После успешного check
на `main` (push или ручной Run workflow) собирает Docker-образ linux/amd64,
публикует его с тегом commit SHA и обновляет существующий сервис Cloud Run.
Деплои выполняются последовательно; pull request не получает OIDC-доступ.

В Settings → Secrets and variables → Actions → Variables нужны:
`GCP_PROJECT_ID`, `GCP_REGION`, `GAR_REPOSITORY`, `CLOUD_RUN_SERVICE`,
`GCP_WORKLOAD_IDENTITY_PROVIDER`, `GCP_DEPLOY_SERVICE_ACCOUNT`, `GCP_RUNTIME_SERVICE_ACCOUNT`.
Авторизация — OIDC, JSON-ключ не нужен. Аккаунту CI нужны Cloud Run Developer
на сервисе, Artifact Registry Writer на Docker-репозитории и Service Account User
на аккаунте приложения. Сервис и OIDC создаются заранее администратором.

CI не меняет IAM или публичный доступ. После первого деплоя администратор
открывает доступ к API отдельно. URL и образ выводятся в Actions Summary.
Успешный деплой подтверждает запуск контейнера, но не готовность кэша:
проверить `/healthz`, затем `/v1/deals` и `/v1/deals/{address}/events`.
До первого снимка API возвращает 503, при закрытом IAM анонимный запрос — 403.

Для фонового обновления устанавливаем 1 CPU, 512 MiB, минимум и максимум
по одному экземпляру, CPU выделяется вне запросов. Это оплачивается и при простое.
Кэш у каждого экземпляра свой; после перезапуска собирается заново.
Откат: повторно развернуть предыдущий образ с тегом SHA через администратора.

Проверьте `/` и `/healthz` по URL, возвращённому gcloud.
Лимит экземпляров ограничивает масштабирование, но не является DDoS-защитой
или гарантией ограничения расходов. Защиту проектируем отдельно.
Для сборки локально нужен Docker: `docker build -t monitoring .`.
Cloud Run требует Linux x86_64: на Apple Silicon используйте
`docker buildx build --platform linux/amd64 --load -t monitoring .`.
