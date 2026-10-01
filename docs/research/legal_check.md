# Правовая проверка — Art Window

Обновлено: 2026-10-01. Рабочие заметки по открытым источникам, не юридическое заключение.

## Главное

- Блокирующих находок нет: 🔴 0 / 🟠 9 / 🟡 13. Ни один открытый первичный текст не запрещает проект в описанном виде; лицензии зависимостей и условия четырёх музеев совместимы с бесплатным распространением.
- 🟠 ближе всего к делу сейчас: в корне репозитория нет файла лицензии — без него поле `license` в `Cargo.toml` может не дать другим права копировать и распространять код. Закрывается двумя файлами.
- 🟠 для промо: рамки iPhone и MacBook в готовом ролике взяты не из набора Apple, а правила Apple требуют своих рамок и запрещают показывать домашний экран. Проверка спроса H1 идёт на Android и этих рамок не касается; готовый ролик с ними — касается.
- 🟠 для каналов распространения: Google Play требует закрытого теста с 12 тестерами на 14 дней; обязательная проверка разработчиков Android к 2027 году коснётся и APK с GitHub; для iOS нужно заранее назвать отличие от других обойных приложений.
- Не закрыто этой проверкой и требует ручного шага: товарные знаки ART WINDOW (USPTO, EUIPO), условия NGA и SMK по прямым ссылкам на изображения, происхождение восьми картин в `promo/assets/`.

## Из чего собрано

| Что | Лицензия / происхождение | Что разрешено | Источник | Вес | Как закрыть |
|---|---|---|---|---|---|
| Сам проект | `MIT OR Apache-2.0` в `Cargo.toml`; файла LICENSE в корне нет | Без явной лицензии «nobody else can copy, distribute, or modify your work»; хватает ли одного поля в манифесте — неизвестно | https://choosealicense.com/no-permission/ ; `Cargo.toml` | 🟠 | Добавить `LICENSE-MIT` и `LICENSE-APACHE` в корень. Это же закрывает `[развилка]` L2 в брифе, если решение — открытый код |
| ~270 крейтов Rust, AndroidX, Compose, kotlinx-coroutines | MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode-3.0 (`cargo metadata`) | Распространение разрешено; MIT требует включать текст уведомления «in all copies», Apache-2.0 §4 — копию лицензии и NOTICE | https://opensource.org/license/mit ; https://www.apache.org/licenses/LICENSE-2.0.txt | 🟡 | Сводный файл сторонних лицензий в релизах и экран «Licences» в приложениях (cargo-about; для Android — плагин лицензий Gradle) |
| `option-ext` 0.2.0 | MPL-2.0 | Копилефт на уровне файлов: §3.3 разрешает включать в работу под другой лицензией; для неизменённого крейта нужны текст MPL и ссылка на исходники | https://www.mozilla.org/en-US/MPL/2.0/ | 🟡 | Текст MPL-2.0 и ссылка на исходники крейта в том же сводном файле; файлы крейта не править |
| `webpki-roots` 1.0.9 | CDLA-Permissive-2.0 (данные — корневые сертификаты) | Передавать можно, «so long as the Data Recipient makes available the text of this agreement» | https://cdla.dev/permissive-2-0/ | 🟡 | Текст соглашения в сводном файле |
| Каталог: Cleveland Museum of Art, 3 859 строк | Open Access, CC0 только при `share_license_status: "CC0"` | Атрибуция не требуется; товарные знаки музея не передаются, ссылка не означает одобрения | https://openaccess-api.clevelandart.org/ ; https://www.clevelandart.org/open-access | 🟡 | Отбор по `share_license_status == "CC0"` уже есть (`catalogue/sources/cleveland.py:74`). Логотип музея не использовать |
| Каталог: The Met, 4 580 строк | Набор данных — CC0; изображения свободны при отметке Open Access | «Please limit request rate to 80 requests per second». Страница политики изображений не открылась (429) | https://metmuseum.github.io/ ; https://github.com/metmuseum/openaccess | 🟡 | Отбор по `isPublicDomain` уже есть (`src/art/met.rs:29`). Страницу https://www.metmuseum.org/policies/image-resources прочитать вручную |
| Каталог: National Gallery of Art, 2 765 строк | Набор данных — CC0-1.0; изображения в него не входят, открыты те, что музей «believes to be in the public domain» | Правила прямых ссылок на изображения и использования названия — неизвестно: страница условий вернула 403 | https://github.com/NationalGalleryOfArt/opendata | 🟠 | Прочитать https://www.nga.gov/notices/open-access-policy.html вручную; отбор по `openaccess == "1"` уже есть (`catalogue/sources/nga.py:96`). Если прямые ссылки ограничены — кэшировать или спросить музей |
| Каталог: SMK, 1 937 строк | CC0 с 2015 года, с 2024 — Public Domain Mark; метаданные CC0 | Атрибуция — просьба. Лимиты и условия API — неизвестно: страница документации не дала текста | https://commons.wikimedia.org/wiki/Commons:SMK_-_Statens_Museum_for_Kunst | 🟡 | Условия API прочитать вручную; отбор по `public_domain` уже есть (`catalogue/sources/smk.py:103`) |
| Каталог: Wikimedia Commons, 25 строк | «Public domain» на Commons — в США и стране происхождения; «reusers should verify local restrictions», «Use is at your own risk» | Прямые ссылки «allowed… but not generally recommended»; клиент без описательного User-Agent получает 403 | https://commons.wikimedia.org/wiki/Commons:Reuse_of_PD-Art_photographs ; https://commons.wikimedia.org/wiki/Commons:Hotlinking ; https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy | 🟡 | User-Agent с названием и контактом; пометка источника в каталоге уже есть (`wmc`). Убрать эти 25 строк — если нужна максимальная чистота для всех стран |
| `promo/assets/*.jpg`, 8 картин | Происхождение файлов в репозитории не записано | Неизвестно | — | 🟠 | Для каждого файла записать музей-источник и его статус (CC0 / public domain); брать из тех же четырёх музеев |
| `promo/assets/frames/iphone.png`, `macbook.png` | MIT, сторонний репозиторий (`promo/assets/frames/LICENSE`) | Лицензия MIT не даёт прав на внешний вид и знаки Apple. Правила Apple: «Use Apple-provided product bezels only»; относятся ли они к ролику приложения вне App Store — текст не говорит | https://developer.apple.com/app-store/marketing/guidelines/ | 🟠 | Официальные рамки Apple (https://developer.apple.com/design/resources/), нейтральные рамки без бренда или кадры без рамок |
| `promo/assets/frames/pixel.png` | Google Device Art Generator | Запрещено в графике и скриншотах листинга Google Play; для соцсетей и сайта запрета на странице нет, отдельных условий не найдено | https://developer.android.com/distribute/marketing-tools/device-art-generator | 🟡 | Не использовать в листинге Play |
| `promo/out-reel/artwindow-reel.wav` | Сгенерирован в проекте (`promo/reel-score.js`) | Своя дорожка | репозиторий | — | Ничего |

## Где распространяется

| # | Находка | Источник | Уверенность | Вес | Как закрыть |
|---|---|---|---|---|---|
| 1 | Google Play: личный аккаунт, созданный после 2023-11-13, проходит закрытый тест — «minimum of 12 testers… opted in continuously for at least 14 days», затем заявка на публикацию | https://support.google.com/googleplay/android-developer/answer/14151465 | высокая | 🟠 | Собрать 12 тестеров заранее — в том числе из откликнувшихся на проверку спроса |
| 2 | Проверка разработчиков Android: с 2026-09-30 обязательна в Бразилии, Индонезии, Сингапуре и Таиланде, глобально — в 2027; касается и установки вне Play на сертифицированных устройствах | https://android-developers.googleblog.com/2026/06/android-developer-verification.html | высокая | 🟠 | Зарегистрировать имя пакета и ключ подписи до глобального срока; одна подпись для Play и GitHub |
| 3 | App Store, п. 4.3: обойные приложения — категория, где новые заявки принимают, только если они «offer a meaningfully different or improved experience» | https://developer.apple.com/app-store/review/guidelines/ | высокая | 🟠 | Назвать отличие в заявке; спорное — в поддержку платформы. Что iOS разрешает вместо программной установки обоев — не проверено |
| 4 | App Store, п. 5.2.2: контент сторонних сервисов должен быть разрешён их условиями, подтверждение — по запросу | там же | высокая | 🟡 | Ссылки на условия четырёх музеев в заметках к заявке |
| 5 | Apple Developer Program: 99 $ в год, без него нет App Store Connect и TestFlight | https://developer.apple.com/programs/enroll/ | высокая | 🟠 | Решение о бюджете до запуска iOS-версии |

## Регулирование области

| # | Находка | Источник | Уверенность | Вес | Как закрыть |
|---|---|---|---|---|---|
| 1 | Сбор адресов для списка ожидания (GDPR): ст. 13 — при сборе сообщить, кто оператор, контакт, цель и срок хранения; согласие должно отзываться так же легко, как даётся; ст. 32 — защита от постороннего доступа | https://gdpr-info.eu/art-13-gdpr/ ; https://gdpr-info.eu/art-32-gdpr/ ; https://www.edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-052020-consent-under-regulation-2016679_en | высокая | 🟡 | На `site/index.html` политика и основание «согласие» уже есть, список хранится вне корня сайта (`site/subscribe.php`). Сверить, что названы срок хранения и способ отзыва |
| 2 | Репродукции картин в общественном достоянии, ЕС: по ст. 14 Директивы 2019/790 воспроизведение такого произведения не получает новой охраны, если само не оригинально. Текст EUR-Lex не открылся, формулировка — по пересказам | https://pro.europeana.eu/post/article-14-and-the-public-domain-the-state-of-play-across-europe | низкая | 🟡 | Открыть ст. 14 в EUR-Lex вручную; вопрос по конкретной стране — к юристу |
| 3 | Италия, ст. 108 Кодекса культурного наследия: плата за коммерческое воспроизведение итальянских государственных культурных ценностей. Каталог берёт картины у музеев США и Дании | https://www.brocardi.it/codice-dei-beni-culturali-e-del-paesaggio/parte-seconda/titolo-ii/capo-i/sezione-ii/art108.html | низкая | 🟡 | Не добавлять итальянские государственные собрания без чтения их условий |

## Название

| # | Находка | Источник | Уверенность | Вес | Как закрыть |
|---|---|---|---|---|---|
| 1 | Прямых совпадений «Art Window» в Google Play, App Store и вебе не найдено; близкие по смыслу — Artpaper, WindowSight, Artpip. Базы товарных знаков USPTO и EUIPO не проверены: это интерактивные поиски, недоступные для этой проверки | https://play.google.com/store/apps/details?id=com.windowsight.windowsight | низкая | 🟠 | Вручную: https://tmsearch.uspto.gov и https://euipo.europa.eu/eSearch — ART WINDOW / ARTWINDOW, классы 9, 41, 42. При совпадении — к юристу |

## Продвижение

| # | Находка | Источник | Уверенность | Вес | Как закрыть |
|---|---|---|---|---|---|
| 1 | Правила Apple для изображений экрана: «Never show Home Screen». Готовый ролик показывает картину как обои в рамке iPhone | https://developer.apple.com/app-store/marketing/guidelines/ | средняя | 🟠 | В рамке iPhone показывать экран приложения, не домашний экран; либо обходиться без рамок Apple. Ролики на Android это не затрагивает |
| 2 | Музыка в роликах: у бизнес-аккаунтов TikTok и Instagram доступны только коммерческие библиотеки площадок; лицензия такой библиотеки действует внутри своей площадки. Первичные страницы TikTok и Meta не открыты, сведения — из вторичных источников | https://support.google.com/youtube/answer/3376882 ; https://www.soundstripe.com/blogs/tiktok-music-library-explained | низкая | 🟡 | Своя дорожка (`promo/reel-score.js`) или без музыки — один файл на все три площадки |
| 3 | Названия музеев в подписях: условия Cleveland и Met не передают права на знаки и запрещают создавать впечатление одобрения | https://www.clevelandart.org/open-access ; https://github.com/metmuseum/openaccess | высокая | 🟡 | Называть музеи как источник картин, без логотипов и без слов о партнёрстве |
| 4 | «Samsung The Frame» в текстах: правила Samsung для третьих лиц не найдены (страница вернула 404) — неизвестно | — | — | 🟡 | Упоминать только как совместимость, без логотипа, с оговоркой «not affiliated»; условия Samsung найти вручную |
| 5 | Отдельных правил площадок о показе самих картин в рекламе не найдено — неизвестно | — | — | 🟡 | В кадре — только картины из каталога; для каждой записывать источник |

## Не найдено / открытые вопросы

- Товарные знаки ART WINDOW в классах 9, 41, 42 — USPTO, EUIPO, TMview.
- Условия NGA и SMK о прямых ссылках на изображения и о кэшировании; полный текст политики изображений Met.
- Первичный текст ст. 14 Директивы 2019/790 и решения Bridgeman v. Corel.
- Достаточно ли поля `license` в манифесте без файла лицензии.
- Что iOS позволяет приложению делать с обоями и как это согласуется с п. 4.3.
- Первичные правила TikTok и Meta о музыке в промо-роликах; правила Samsung об упоминании The Frame.
