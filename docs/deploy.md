# Cloud Run

Сервис слушает `0.0.0.0:$PORT`, по умолчанию 8080, обрабатывает SIGTERM.
Контейнер запускается от непривилегированного пользователя.
Контракт: https://docs.cloud.google.com/run/docs/container-contract

Для деплоя нужны Google Cloud project ID, регион, авторизация gcloud,
биллинг и доступ к Cloud Run, Cloud Build и Artifact Registry.
Деплой выполняется отдельно, после успешных локальных проверок.

```sh
./scripts/check
gcloud run deploy monitoring \
  --source . \
  --project YOUR_PROJECT_ID \
  --region YOUR_REGION \
  --allow-unauthenticated \
  --min-instances 0 \
  --max-instances 2 \
  --memory 256Mi \
  --port 8080
```

Проверьте `/` и `/healthz` по URL, возвращённому gcloud.
Лимит экземпляров ограничивает масштабирование, но не является DDoS-защитой
или гарантией ограничения расходов. Защиту проектируем отдельно.
Для сборки локально нужен Docker: `docker build -t monitoring .`.
Cloud Run требует Linux x86_64: на Apple Silicon используйте
`docker buildx build --platform linux/amd64 --load -t monitoring .`.
