# Первоначальная настройка деплоя в Cloud Run

Инструкция для `gonka24/monitoring`. Обычный процесс CI описан в [deploy.md](deploy.md).
Команды выполняются в Cloud Shell от человека с соответствующими правами.
При ошибке остановитесь: следующие шаги могут зависеть от невыполненного действия.
Уже существующие ресурсы сначала проверяйте; не удаляйте их ради повторного запуска.

## Ресурсы и участники

| Ресурс | Значение |
|---|---|
| Проект / номер | `anikiyevichm-gnk` / `775629696593` |
| Регион | `us-central1` |
| Сервис Cloud Run | `monitoring` |
| Docker-репозиторий | `cloud-run-images` (существующий, общий с другими образами) |
| Пул / провайдер OIDC | `monitoring-github` / `github` |
| GitHub | `gonka24/monitoring`, repository ID `1419130687`, owner ID `336940991` |
| Аккаунт CI | `monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com` |
| Аккаунт приложения | `monitoring-runtime@anikiyevichm-gnk.iam.gserviceaccount.com` |

Существующие `github-pool` и `github-deploy` обслуживают другие проекты и не используются.
OIDC допускает только наш репозиторий, его владельца, ветку main и события push/workflow_dispatch.
JSON-ключи и GitHub Secrets для этой схемы не нужны.

### Постоянные права CI и приложения

| Кому | Роль | На каком ресурсе | Зачем |
|---|---|---|---|
| GitHub principalSet нашего repository ID | `roles/iam.workloadIdentityUser` | Аккаунт `monitoring-deploy` | Получать временные credentials через OIDC |
| `monitoring-deploy` | `roles/run.developer` | Только сервис `monitoring` | Обновлять сервис и трафик |
| `monitoring-deploy` | `roles/artifactregistry.writer` | Репозиторий `cloud-run-images` | Публиковать и читать образы; доступ ко всему этому репозиторию |
| `monitoring-deploy` | `roles/iam.serviceAccountUser` | Аккаунт `monitoring-runtime` | `iam.serviceAccounts.actAs`: запускать ревизию от имени приложения |
| `monitoring-runtime` | Не требуются | — | Приложение читает публичную ноду, не использует Google Cloud API |
| `allUsers` (после открытия API) | `roles/run.invoker` | Только сервис `monitoring` | Анонимно вызывать API |

Google также управляет Cloud Run service agent:
`service-775629696593@serverless-robot-prod.iam.gserviceaccount.com`.
Его системная роль `roles/run.serviceAgent` назначается платформой на проекте.
Не заменяйте им runtime и не выдавайте роли service agent людям или CI.
Cloud Build в workflow не используется: Docker собирается на GitHub runner.

### Какие права нужны человеку-настройщику

Это роли для разных операций, а не требование выдать всё одному человеку.
Администратор может выполнить соответствующий шаг сам. Роль Creator не разрешает
менять IAM созданного ресурса; роль с названием «IAM Admin» нужно проверять по точному ID.

| Операция человека | Подходящая предопределённая роль | Область |
|---|---|---|
| Включить API | `roles/serviceusage.serviceUsageAdmin` | Проект |
| Создать два аккаунта | `roles/iam.serviceAccountCreator` | Проект |
| Создать пул и провайдер OIDC | `roles/iam.workloadIdentityPoolAdmin` | Проект |
| Выдать Workload Identity User и Service Account User | `roles/iam.serviceAccountAdmin` | Соответствующий аккаунт: deploy или runtime |
| Выдать Writer на образы | `roles/artifactregistry.admin` | Существующий `cloud-run-images` |
| Создать Docker-репозиторий, если отсутствует | `roles/artifactregistry.admin` | Проект; в нашем проекте он уже существует |
| Создать первоначальный сервис | `roles/run.admin` | Проект: нового сервиса ещё нет |
| Изменять IAM и публичный доступ существующего сервиса | `roles/run.admin` | Только `monitoring` |
| При первом запуске назначить runtime | `roles/iam.serviceAccountUser` | `monitoring-runtime` |
| Добавить GitHub Variables | Доступ write или выше к репозиторию GitHub | `gonka24/monitoring` |

Роли человеку выдаёт уполномоченный администратор. Проектный Cloud Run Admin
нужен только участнику, который создаёт сервис; после создания доступ можно сузить.
CI не получает проектную роль Cloud Run Developer и не меняет IAM.
Биллинг должен быть подключён заранее владельцем биллинга; его подключение здесь не описано.

Перед началом проверьте активный аккаунт и проект:

```sh
gcloud auth list --format="table(account,status)"
gcloud projects describe anikiyevichm-gnk --format="yaml(projectId,projectNumber,lifecycleState)"
gcloud billing projects describe anikiyevichm-gnk --format="value(billingEnabled)"
```

## 1. Создать аккаунты, OIDC и привязки IAM

Первый блок создаёт ресурсы IAM и добавляет права. Создание сервиса — в разделе 2.
Выполнять блоки по порядку. При ошибке остановиться и передать её текст.
Если ресурс уже существует, не удалять его: сначала проверить его настройки.

```sh
gcloud services enable run.googleapis.com artifactregistry.googleapis.com iam.googleapis.com iamcredentials.googleapis.com sts.googleapis.com \
  --project=anikiyevichm-gnk

gcloud iam service-accounts create monitoring-deploy \
  --project=anikiyevichm-gnk --display-name="Monitoring GitHub deploy"

gcloud iam service-accounts create monitoring-runtime \
  --project=anikiyevichm-gnk --display-name="Monitoring runtime"
```

```sh
gcloud iam workload-identity-pools create monitoring-github \
  --project=anikiyevichm-gnk --location=global \
  --display-name="Monitoring GitHub"

gcloud iam workload-identity-pools providers create-oidc github \
  --project=anikiyevichm-gnk --location=global \
  --workload-identity-pool=monitoring-github \
  --issuer-uri=https://token.actions.githubusercontent.com \
  --attribute-mapping="google.subject=assertion.sub,attribute.repository_id=assertion.repository_id" \
  --attribute-condition="assertion.repository_id == '1419130687' && assertion.repository_owner_id == '336940991' && assertion.repository == 'gonka24/monitoring' && assertion.ref == 'refs/heads/main' && (assertion.event_name == 'push' || assertion.event_name == 'workflow_dispatch')"

gcloud iam service-accounts add-iam-policy-binding \
  monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com \
  --project=anikiyevichm-gnk \
  --role=roles/iam.workloadIdentityUser \
  --member="principalSet://iam.googleapis.com/projects/775629696593/locations/global/workloadIdentityPools/monitoring-github/attribute.repository_id/1419130687"
```

Права GitHub ограничиваем сервисом `monitoring`, репозиторием образов
`cloud-run-images` и аккаунтом приложения `monitoring-runtime`.
Аккаунт приложения не получает ролей Google Cloud: он читает публичную ноду.
Следующие команды выполняет пользователь с правами изменения IAM соответствующих ресурсов.
Запись в `cloud-run-images` распространяется на весь этот Docker-репозиторий.

```sh
gcloud artifacts repositories add-iam-policy-binding cloud-run-images \
  --project=anikiyevichm-gnk --location=us-central1 \
  --member="serviceAccount:monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com" \
  --role=roles/artifactregistry.writer

gcloud iam service-accounts add-iam-policy-binding \
  monitoring-runtime@anikiyevichm-gnk.iam.gserviceaccount.com \
  --project=anikiyevichm-gnk \
  --member="serviceAccount:monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com" \
  --role=roles/iam.serviceAccountUser
```

## 2. Администратор создаёт сервис и выдаёт доступ CI

Если сервис `monitoring` уже существует, не выполнять команду создания с hello-образом:
сначала проверить существующий сервис. Команда ниже предназначена для нового сервиса.
Она создаёт временную закрытую заглушку из официального образа Google.
Это ещё не наше приложение. Минимум экземпляров — 0, до настройки CI постоянный
фоновый сбор не запускается. Первый деплой из CI заменит образ нашим приложением.
Администратору нужны права создания Cloud Run сервиса и использования `monitoring-runtime`.

Для первоначального запуска человеку тоже нужен `actAs` на runtime.
Администратор аккаунта выдаёт его фактическому оператору (подставьте его email):

```sh
gcloud iam service-accounts add-iam-policy-binding \
  monitoring-runtime@anikiyevichm-gnk.iam.gserviceaccount.com \
  --project=anikiyevichm-gnk \
  --member="user:OPERATOR_EMAIL" --role=roles/iam.serviceAccountUser
```

```sh
gcloud run deploy monitoring \
  --project=anikiyevichm-gnk --region=us-central1 \
  --image=us-docker.pkg.dev/cloudrun/container/hello \
  --service-account=monitoring-runtime@anikiyevichm-gnk.iam.gserviceaccount.com \
  --min=0 --max=1 --memory=512Mi --port=8080 \
  --no-allow-unauthenticated --quiet

gcloud run services add-iam-policy-binding monitoring \
  --project=anikiyevichm-gnk --region=us-central1 \
  --member="serviceAccount:monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com" \
  --role=roles/run.developer

gcloud run services get-iam-policy monitoring \
  --project=anikiyevichm-gnk --region=us-central1 --format=json
```

Если появляется `Permission denied`, передать команду администратору с правами
создания сервиса/изменения его IAM. Область доступа сужена, но проверка прав не обходится.

CI будет обновлять существующий `monitoring`, без изменения его IAM.
После выкладки настоящего приложения администратор один раз откроет публичный доступ:

```sh
gcloud run services add-iam-policy-binding monitoring \
  --project=anikiyevichm-gnk --region=us-central1 \
  --member=allUsers --role=roles/run.invoker
```

Если политика организации запрещает `allUsers`, остановиться и согласовать доступ
с администратором. До открытия доступа анонимный запрос к URL сервиса ожидаемо запрещён.

## 3. Добавить GitHub Variables

Открыть https://github.com/gonka24/monitoring/settings/variables/actions
(Settings → Secrets and variables → Actions → Variables → New repository variable).

| Name | Value |
|---|---|
| GCP_PROJECT_ID | anikiyevichm-gnk |
| GCP_REGION | us-central1 |
| GCP_WORKLOAD_IDENTITY_PROVIDER | projects/775629696593/locations/global/workloadIdentityPools/monitoring-github/providers/github |
| GCP_DEPLOY_SERVICE_ACCOUNT | monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com |
| GCP_RUNTIME_SERVICE_ACCOUNT | monitoring-runtime@anikiyevichm-gnk.iam.gserviceaccount.com |
| GAR_REPOSITORY | cloud-run-images |
| CLOUD_RUN_SERVICE | monitoring |

Secrets и JSON-ключи для этой схемы не нужны.
Сообщить, какие блоки выполнены и добавлены ли переменные; при ошибках передать текст.
Изменения IAM могут применяться несколько минут.

## 4. Запустить CI и проверить приложение

Workflow: проверки → сборка linux/amd64 → публикация образа с тегом commit SHA → Cloud Run.
Деплой только из main; OIDC разрешает push и ручной workflow_dispatch.
Для минутного сборщика планируем instance-based billing и минимум один экземпляр:
это означает расходы даже без HTTP-трафика. Кэш остаётся в памяти каждого экземпляра
и после перезапуска заполняется заново. Доступность процесса и готовность кэша проверяем отдельно.

В GitHub Actions откройте workflow **Rust → Run workflow → main**.
При ошибке после исправления IAM используйте **Re-run failed jobs**; применение
IAM может занять несколько минут. В Summary успешного deploy будут URL и образ.
CI обновляет только существующий сервис, затем переводит трафик на latest.
Успех job не доказывает готовность кэша: workflow не проверяет HTTP API.

После публикации и открытия публичного доступа выполните:

```sh
monitoring_url=$(gcloud run services describe monitoring \
  --project=anikiyevichm-gnk --region=us-central1 --format='value(status.url)')
curl --fail-with-body "$monitoring_url/healthz"
curl --fail-with-body "$monitoring_url/v1/deals"
curl --fail-with-body "$monitoring_url/v1/deals/gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs/events"
```

`/healthz` проверяет процесс. До первого снимка `/v1/deals` возвращает 503;
после прогрева проверьте `updated_at` и `stale=false`. 403 означает, что нужно
проверить IAM/ingress, а не менять код обработчика. Каждый экземпляр имеет свой кэш.

## 5. Диагностика прав

Смотрите аккаунт, указанный в ошибке: пользователь Cloud Shell и аккаунт CI — разные участники.
Отказы могут также означать неверное имя/отсутствие ресурса или организационные ограничения.

| Ошибка | Что проверить |
|---|---|
| OIDC attribute condition rejected | Пул/провайдер, ID репозитория/владельца, main, событие push или workflow_dispatch |
| `iam.serviceAccounts.getAccessToken` | Workload Identity User для правильного principalSet на deploy |
| `artifactregistry.repositories.uploadArtifacts` | Writer для deploy на `cloud-run-images` |
| `artifactregistry.repositories.getIamPolicy` / `setIamPolicy` | Права человека менять IAM репозитория; обратиться к его администратору |
| `run.services.get` / `update` | Наличие сервиса и Cloud Run Developer для deploy на нём |
| `run.services.setIamPolicy` | Cloud Run Admin у человека на сервисе; CI эту операцию не выполняет |
| `iam.serviceAccounts.actAs` | Service Account User у действующего deploy/оператора на runtime |
| `iam.serviceAccounts.getIamPolicy` / `setIamPolicy` | Service Account Admin у человека на соответствующем аккаунте |
| `resourcemanager.projects.setIamPolicy` | Выдача ролей на проекте; нашему CI она не нужна |

Администраторы соответствующих ресурсов могут проверить прямые привязки:

```sh
gcloud iam service-accounts get-iam-policy monitoring-deploy@anikiyevichm-gnk.iam.gserviceaccount.com --project=anikiyevichm-gnk --format=json
gcloud iam service-accounts get-iam-policy monitoring-runtime@anikiyevichm-gnk.iam.gserviceaccount.com --project=anikiyevichm-gnk --format=json
gcloud artifacts repositories get-iam-policy cloud-run-images --project=anikiyevichm-gnk --location=us-central1 --format=json
gcloud run services get-iam-policy monitoring --project=anikiyevichm-gnk --region=us-central1 --format=json
```

Эти политики не показывают все наследуемые разрешения и запреты.
Если привязка есть, проверьте IAM Conditions и Policy Troubleshooter.
Не присылайте JSON-ключи или access tokens; для диагностики достаточно ошибки и привязок.

## 6. Откат

Выберите ранее успешно развёрнутый SHA в Actions Summary; убедитесь, что образ
с этим тегом существует. Оператору нужны Developer на сервисе, Reader на репозитории
и Service Account User на runtime. Откат меняет образ, но сохраняет текущую конфигурацию.

```sh
previous_sha=REPLACE_WITH_PREVIOUS_SUCCESSFUL_COMMIT_SHA
gcloud run services update monitoring \
  --project=anikiyevichm-gnk --region=us-central1 \
  --image="us-central1-docker.pkg.dev/anikiyevichm-gnk/cloud-run-images/monitoring:$previous_sha" --quiet
gcloud run services update-traffic monitoring \
  --project=anikiyevichm-gnk --region=us-central1 --to-latest --quiet
```

Не выполняйте ручной откат параллельно с deploy job. Исправьте или откатите код в main,
иначе следующий push снова развернёт новую версию. После отката повторите HTTP-проверки.

## Источники

- [Настройка Workload Identity Federation](https://docs.cloud.google.com/iam/docs/workload-identity-federation-with-deployment-pipelines)
- [Управление IAM сервисных аккаунтов](https://docs.cloud.google.com/iam/docs/manage-access-service-accounts)
- [Создание сервисных аккаунтов](https://docs.cloud.google.com/iam/docs/service-accounts-create)
- [Включение API](https://docs.cloud.google.com/service-usage/docs/enable-disable)
- [Права Artifact Registry](https://docs.cloud.google.com/iam/docs/roles-permissions/artifactregistry)
- [Деплой Cloud Run](https://docs.cloud.google.com/run/docs/deploying)
- [Аккаунт приложения и actAs](https://docs.cloud.google.com/run/docs/configuring/services/service-identity)
