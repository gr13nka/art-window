# Исследование рынка — Art Window

Обновлено: 2026-09-29. Рабочие заметки, не единица ALPS; материал для problem_analysis, brief и market_analysis.

## Главное
- **Самое сильное против:** ни одно отличие Art Window не защищено копированием. «Целиком с полями» (D1) — режим fit, который есть в любой ОС; «все Spaces» (D2) уже есть у Irvue, 24 Hour Wallpaper, Wallspace, BingWallpaper-for-Mac; мульти-музейный каталог (D4) — у MP Art (790K работ) и Daily Canvas (2 700+).
- **Но D1 никто не хочет копировать:** Daily Canvas режет «to the focal point», Frame Crop — «without letterboxing», Artpaper v4 — «fills edge to edge», Muzei размывает. Граница держится на чужих стимулах, не на трудности.
- **За категорию не платят** (кладбище: Artpip, Museum Art Wallpapers, Meural, Panels; Artpaper на Mac теперь Free). Платят рядом: Samsung Frame (Art Store $49.99/год, Frame Crop, MP Art, Etsy-бандлы) и DailyArt (контент-подписка).
- **Боль «режут картину»** видна у владельцев Samsung Frame и в тикетах Muzei (#409, #695, #697); на Android кроп — свойство API живых обоев (мейнтейнер Muzei, #711). Боль «один Space» — почти только голоса разработчиков.
- **Риск техники:** Apple Developer Forums (02.2026, Tahoe) — macOS сам возвращает per-space режим; правка базы Dock хрупка.
- **Reddit (Arctic Shift, 2026-09-29) сдвинул три вывода:** (1) на Samsung Frame голые чёрные поля не просят — просят убрать навязанное паспарту и заполнить экран (8 тредов) или выбрать паспарту самому (6 тредов); (2) боль «обои только на одном Space» есть и у пользователей, не только у разработчиков (3 треда в 2026); (3) на размытие в Muzei жалоб не нашлось — жалуются на зум и отсутствие центрирования.
- **Канал назван:** r/TheFrame тепло принимает бесплатные инструменты от авторов (40–70 очков), r/macapps и r/gnome принимают запуски обойных приложений. На Frame уже 8+ бесплатных DIY-инструментов, в том числе с ротацией из Рейксмузеума (SAWSUBE).
- 2026-09-29: модель сменена на бесплатную («ad for other my projects») — вопрос «кто заплатит за Art Window» снят, вопрос охвата стал главным.

## Проверка утверждений
| Claim | Было | Стало | Основание |
|---|---|---|---|
| C1. Раздражает, что картину режут/размывают | выстрадано (автор) | + косвенный сигнал (Muzei #409/#695/#697, Samsung Community) | Боль, Т2 #1–5 |
| C2. Обои на Mac меняются лишь на одном Space — это боль пользователей | выстрадано (автор) | косвенный сигнал: 3 пользовательских треда r/MacOS 2026; конкуренты уже решили | Боль, Т2 #6–9; Reddit Q3 |
| C4. Владельцы ТВ хотят картину целиком с полями | допущение | против для чёрных полей: хотят заполнить экран или выбрать паспарту | Reddit Q1 |
| C3. Кто-то заплатит за daily-art обои | допущение | против: кладбище; за — только Frame TV | Цены |

## Альтернативы и конкуренты
| # | Игрок | Что делает / цена / тракшн | Держит дифференциатор | Может скопировать отличие Art Window | Источник | Дата | Увер. |
|---|---|---|---|---|---|---|---|
| 1 | Artpaper | macOS/iOS/Android; 1000+ картин; Mac — Free (v4.0.0, «fills edge to edge»), на gikken.co — «$9.99 pay once»; Android 100K+ (сниппет) | отобранные сканы, 5K, мультимонитор | D1 — да, за день, но v4 ушёл в edge-to-edge; D4 — да | https://apps.apple.com/us/app/artpaper-new-wallpapers-daily/id1028838684?mt=12 ; https://gikken.co/artpaper/mac/ | авг 2026 | средняя |
| 2 | Daily Canvas | iOS/Mac/TV; 2 700+ картин (Met, Cleveland, AIC); Pro $9.99/год; «cropped to the focal point»; не запущен | фокальный кроп, рассказ о картине | D1 — может, но идеология противоположная; D4 — да; Android нет | https://dailycanvasapp.com/ | 2026 | средняя |
| 3 | MP Art | Apple-платформы; 790K+ работ из 5 музеев; Premium $9.99/год; отправка на Frame, 21+ матов | мульти-музейный каталог + ТВ | D4 — уже; D1 — частично (маты); обои Mac/Android не ставит | https://apps.apple.com/us/app/mp-art/id6757153314 | 2026 | средняя |
| 4 | Muzei | Android, бесплатно, Apache-2.0, 4.9K★; 1M+ (сниппет; AppBrain 2.4M), 3.80★ по 46K оценок (сниппет); размывает по умолчанию; v3.7.1 09.09.2026 | стандарт арт-обоев Android, плагины | D3 — плагин мог бы, ядро не менялось; D1 — нет (идея blur); кроп — ограничение API (#711) | https://github.com/muzei/muzei ; https://github.com/muzei/muzei/discussions/711 | 2026 | высокая |
| 5 | Irvue | macOS, Unsplash-фото; Free + $7/год (источники расходятся); v2026.3 | каналы фото, «rock solid» | D2 — уже (опция всех Spaces, AlternativeTo) | https://apps.apple.com/us/app/irvue-desktop-wallpapers/id1039633667?mt=12 ; https://alternativeto.net/software/irvue/ | 03.2026 | средняя |
| 6 | 24 Hour Wallpaper | macOS; $1.49 за обои / $29.99 навсегда; одна картинка на все Spaces | обои по времени суток | D2 — уже | https://www.jetsoncreative.com/24hourspaces | 01.2026 | высокая |
| 7 | BingWallpaper-for-Mac | open source, 60★; все мониторы и Spaces | простота | D2 — уже | https://github.com/2h4u/BingWallpaper-for-Mac | неизв. | средняя |
| 8 | macOS штатно | Fit to Screen с полями; «Show on all Spaces»; shuffle папки; жалобы на shuffle в Tahoe | дефолт ОС | D1/D2 — есть вручную; каталога нет | https://discussions.apple.com/thread/256152434 ; https://www.wallpaperyapp.com/troubleshooting-common-mac-wallpaper-problems | 2026 | средняя |
| 9 | Google Arts & Culture (Chrome) | картина на новой вкладке; 200K пользователей, 4.5★; v3.1.0 03.2026. ChromeOS «Wallpaper Art» (2015) — статус неизвестен | бренд и 70+ музеев | D4 — да; обои не меняет; дневной обойный продукт запускал и не развивал | https://chromewebstore.google.com/detail/google-arts-culture/akimgimeeoiognljlfchpbkpfbmeapkh ; https://9to5google.com/2015/11/11/googles-new-wallpaper-art-app-puts-beautiful-artwork-on-your-chromebook/ | 2026 / 2015 | высокая |
| 10 | Variety (Linux) | open source, 1.7K★; Unsplash, Bing, Wallhaven, папки | стандарт менялки на Linux | D5, D6 — уже; курируемых картин и fit как идеи нет | https://github.com/varietywalls/variety/releases | 2026 | высокая |
| 11 | GNOME daily-art мелочь | WikiArt Wallpaper (0★), Picture of the Day (321 загрузка); Bing Wallpaper extension — 495 764 загрузки | картинка дня в панели | D5 — любой может; стимула нет | https://extensions.gnome.org/extension/1262/bing-wallpaper-changer/ ; https://extensions.gnome.org/extension/10000/picture-of-the-day/ | 2026 | высокая |
| 12 | Frame Crop | iOS/Android/Desktop; Mac $9.99 разово; 360K+ скачанных работ, 4.75★ (750+, самоотчёт); кроп под Frame «without letterboxing» | «отмени подписку Samsung» | D1 — противоположная позиция | https://framecrop.app/ ; https://framecrop.app/desktop | неизв. | средняя |
| 13 | Samsung Art Store | Frame TV; $4.99/мес / $49.99/год | экосистема ТВ | — (другой экран) | https://www.frametvartist.com/blog/samsung-art-store-subscription-review | 05.2026 | высокая |
| 14 | Etsy «Frame TV art» | бандлы 1 000–32 000 PD-картин, $1.91–36.55 (сниппет) | дёшево за тысячи файлов 16:9 | — | https://www.etsy.com/market/frame_tv_art_bundle | неизв. | низкая |
| 15 | DailyArt | контент о картине дня; 6.1M скачиваний Android; ~$20k/мес (Similarweb, 05.2022); +168% после подписки | история искусства | — (не обои) | https://www.hologramdesign.co/case-study/dailyart-app | 2022–2025 | средняя |
| 16 | Wallpaper Engine | Windows/Steam €4.99, 233K отзывов 97% | живые обои | нет, другая категория | https://store.steampowered.com/app/431960/Wallpaper_Engine/ | 2026 | высокая |
| 17 | Кладбище | Artpip (Pro $10, discontinued; сайт лежит — наблюдение автора); Museum Art Wallpapers (заброшен 05.2020); Meural (закрыт 09.05.2024, «маленький рынок»); Panels (закрыт 31.12.2025) | — | — | https://alternativeto.net/software/artpip/about/ ; https://apps.apple.com/us/app/museum-art-wallpapers/id1507427210 ; https://www.channelnews.com.au/exclusivenetgear-kills-off-meural-canvas/ ; https://9to5mac.com/2025/12/01/mkbhd-is-shutting-down-his-iphone-wallpaper-app/ | 2017–2025 | высокая |
| 18 | Backgroundifier | 2016: картины в обои с полями (не открыт) | идея «картина с полями» | D1 — идея была ещё в 2016 | http://archagon.net/blog/2016/09/19/turning-your-macos-desktop-into-a-rotating-art-gallery-with-backgroundifier/ | 2016 | низкая |
| 19 | samsung-frame-art-gallery | MIT, 0★; фильтр формы 1.4–1.9, ≥1900 px, центр-кроп | — | D3 — аналогичный фильтр формы уже написан | https://github.com/robpearlman/samsung-frame-art-gallery | 2026 | средняя |

**Копируемость по отличиям.**
- D1 «целиком, с полями» — копируется за день, но три ближайших игрока сознательно выбрали кроп; держится на их стимулах.
- D2 «все Spaces» — не отличие: уже у четырёх игроков; к тому же хрупко на Tahoe (https://developer.apple.com/forums/thread/814926, 02.2026).
- D3 «фильтр формы на Android» — копируется дёшево; не сделано ни Muzei, ни Artpaper.
- D4 «мульти-музейный каталог ≥ 2000 px» — не отличие: MP Art, Daily Canvas, Artvee.
- D5 «GNOME» — поле пустое по качеству, занято мелочью; денег и стимула там нет ни у кого.
- D6 «open source, без аккаунта» — условие входа, не отличие (Muzei, Variety).

## Боль в открытых источниках
| # | Находка | Источник | Дата | Увер. | Claim |
|---|---|---|---|---|---|
| 1 | Muzei #409: «Why does the app zoom into the picture instead of taking the whole picture for wallpaper? The preview works well.» | https://github.com/muzei/muzei/issues/409 | 2017 | высокая | C1 + |
| 2 | Muzei #697: «by default, it remains in a position where I cannot see the central beauty of the images I have» | https://github.com/muzei/muzei/issues/697 | 11.2020 | высокая | C1 + |
| 3 | Muzei #695: просят выбирать, где центрировать/обрезать | https://github.com/muzei/muzei/issues/695 | 10.2020 | высокая | C1 + (выбор кропа, не «целиком») |
| 4 | Samsung Community «Art in Art Store is cropped»: подписчик против «только верхней половины Моны Лизы», грозит отпиской (сниппет) | https://eu.community.samsung.com/t5/tv/art-in-art-store-is-cropped/td-p/4058564 | 11.2024 | средняя | C1 + (платящий) |
| 5 | Frame 2019: маты режут «feet/heads» (сниппет) | https://eu.community.samsung.com/t5/tv/the-frame-2019-needs-some-improvements/m-p/1280256/highlight/true | 2019 | низкая | C1 + |
| 6 | 16% негативных отзывов Zedge/Walli/Backdrops — «screen fit / cropping» (не арт) | https://unstar.app/blog/zedge-walli-backdrops-vellum-wlppr-wallpaper-apps-ranked-2026 | 2026 | средняя | C1 + косвенно |
| 7 | Tahoe: после скриптовой смены «Show on all spaces» выключается сам — голос разработчика | https://developer.apple.com/forums/thread/814926 | 02.2026 | высокая | C2 + (технич.) |
| 8 | Чёрные обои при переключении Spaces в Tahoe, 31 «Me too» — баг ОС | https://discussions.apple.com/thread/256143036 | 09.2025 | средняя | C2 косвенно |
| 9 | Жалобы на Spaces в Apple Communities — про другое (TopNotch, терминология) | https://discussions.apple.com/thread/255159214 ; https://discussions.apple.com/thread/256133188 | 2023–2025 | средняя | C2 − |
| 10 | Отзывы Artpaper хвалят разные картины на разных экранах, жалоб на кроп/Spaces нет | https://apps.apple.com/us/app/artpaper-new-wallpapers-daily/id1028838684?mt=12 | 2026 | средняя | C2 − |
| 11 | Наблюдение автора: «muzei on android provides blury images in poor quality», «art pip is ugly and its website is down» | interview_log 2026-09-29 | 2026 | — (выстрадано) | C1 + |
| 12 | Reddit недоступен инструментам ресёрча — тишина там не находка, а пробел | — | 2026-09-29 | — | — |

## Размер и сегменты
- Samsung Frame: 1M+ продано за 2021, 2M+ накопленно ожидалось (https://www.flatpanelshd.com/news.php?subaction=showfull&id=1637823840, 2021, высокая); цель «5 млн» — не проверена.
- Muzei: 1M+ / 2.4M загрузок (сниппет/AppBrain) — ёмкость «арт на обоях Android», низкая уверенность.
- GNOME: Bing Wallpaper extension — 495 764 загрузки (https://extensions.gnome.org/extension/1262/bing-wallpaper-changer/, 2026, высокая).
- Mac с несколькими мониторами/Spaces — доля неизвестна (анекдот HN: https://news.ycombinator.com/item?id=21648178, 2019, низкая).
- r/Art 22.3M (https://en.wikipedia.org/wiki/R/Art, 11.2025); r/TheFrame, r/macapps и др. — неизвестно.
- Тренды: Art Store на OLED Samsung S95H/S90H, генеративные AI-обои (https://www.arttv.ai/guides/samsung-art-store, 2026); открытый доступ музеев растёт (Met ~492K изображений, сниппет).

## Цены и модели
- Mac-обои: Wallcat free, Irvue free/$7 год, Solace $9.99, Wallspace Pro $12.99, Wallper $14.99, 24 Hour Wallpaper $29.99 навсегда (https://www.theodorehq.com/solace/blog/posts/best-wallpaper-apps-mac, 08.2026, высокая).
- Мобильные: free + реклама + Pro; 18% негативных отзывов — на paywall (unstar.app, 2026).
- Frame TV: Art Store $49.99/год; Frame Crop Mac $9.99 разово («I'd pay $25 a year for this and stop paying Samsung!», самоотбор); Etsy-бандлы $1.91–36.55.
- Подписки на арт-обои: Daily Canvas $9.99/год (не запущен), MP Art $9.99/год, Museum Art Wallpapers $15.99/год (заброшен), Panels $49.99/год (закрыт).
- Донаты: Wallpaperer (tip jar) — оценок слишком мало для рейтинга (https://apps.apple.com/us/app/id1102248738, 2025).
- Indie в целом: 50% < $1K/мес (https://techstartups.com/2026/09/28/what-percentage-of-indie-hacker-products-make-100k-in-arr/, 2026).

## Не найдено / открытые вопросы
- Reddit не проверены: r/SamsungFrame, r/wallpapers, r/androidthemes, r/Art, r/mac, r/ArtHistory, r/unixporn (таймауты или не запрашивались). Правила самопродвижения сабреддитов не читались.
- Вывод на телевизор в коде строится для Android/Google TV (leanback, TvActivity, ArtDream — незакоммичено); Samsung Frame на Tizen этим не покрыт.
- Play Store: установки и 1–2★ отзывы Artpaper, Muzei; IAP в Artpaper «Free».
- Есть ли у Art Window дорога в F-Droid, Flathub, extensions.gnome.org — где стоят Muzei и Bing Wallpaper.
- Какие «другие проекты» рекламирует Art Window и что они продают.

## Reddit (Arctic Shift API, 2026-09-29)
Путь: `arctic-shift.photon-reddit.com` — `query=` внутри сабреддита + комментарии по `link_id`. ~75 запросов, много таймаутов; очки комментариев 1–9 — это отдельные голоса, не голосование. Отсутствие находки — слабое свидетельство.

### Samsung Frame (r/TheFrame)
| # | Находка | Тред | Дата | Очки/комм. | Claim |
|---|---|---|---|---|---|
| 1 | «I can't select 'No mat'» — ответ: «I bought the app FrameCrop because I was sick of this» | https://www.reddit.com/r/TheFrame/comments/1v7deey/ | 2026-07-26 | 2/9 | C4 − |
| 2 | «how to remove this permanently and get photos to fill the screen?» — «Frame Crop 100% solves this problem» | https://www.reddit.com/r/TheFrame/comments/1oea1qa/ | 2025-10-23 | 6/13 | C4 − |
| 3 | «Some need cropping, and I don't want mats on any of them» | https://www.reddit.com/r/TheFrame/comments/1mnqepm/ | 2025-08-11 | 1/0 | C4 − |
| 4 | Не-16:9 «only get a mat … Make sure Art is exactly 3840x2160»; «The forced matte options when importing in bulk is a pain»; повёрнутый Frame «shows them in huge letterbox mode» | https://www.reddit.com/r/TheFrame/comments/1l3567v/ | 2025-06-04 | 29/18 | C4 − |
| 5 | Reframed.gallery: картины доработаны до 16:9, «not cropped or scaled»; бесплатно + tip jar | https://www.reddit.com/r/TheFrame/comments/1n111tl/ | 2025-08-27 | 37/9 | новое: целиком через дорисовку, не поля |
| 6 | «All or nothing removal of matte is no good» (опрос Frame Crop) | https://www.reddit.com/r/TheFrame/comments/1ll1geq/ | 2025-06-26 | 2/3 | C4 − |
| 7 | Бесплатные кропперы 16:9 (NGA cropper, Wall Art Frame) | https://www.reddit.com/r/TheFrame/comments/1pt4boy/ ; https://www.reddit.com/r/TheFrame/comments/1uhxie3/ | 2025-12 / 2026-06 | 72/23 ; 17/4 | C4 − |
| 8 | SAWSUBE: пушит на ТВ, ротация по расписанию из Рейксмузеума; портреты «blur-filled like Instagram» | https://www.reddit.com/r/TheFrame/comments/1stklh3/ | 2026-04-23 | 69/17 | новое: конкурент D7 |
| 9 | «Mat designer in the works!» — «shocked at how few mat color choices there are in the samsung software» | https://www.reddit.com/r/TheFrame/comments/1v867cq/ | 2026-07-27 | 36/17 | C4 + (паспарту) |
| 10 | Бесплатные PSD-паспарту; жалоба, что Samsung убрал сохранение своих паспарту | https://www.reddit.com/r/TheFrame/comments/1v2116h/ | 2026-07-21 | 9/9 | C4 + |
| 11 | «I want [it] to default to placing the art on a mat» — «not all paintings should have the same mat … 15 different widths and colors» | https://www.reddit.com/r/TheFrame/comments/1nes818/ | 2025-09-12 | 2/6 | C4 + (паспарту под работу) |
| 12 | «I love the Frame in that it has virtual 'fake' matting»; «How to get this type of matte» | https://www.reddit.com/r/TheFrame/comments/1vr4utl/ ; https://www.reddit.com/r/TheFrame/comments/1uub1j4/ | 2026 | ?/17 ; 10/3 | C4 + |
| 13 | «only SIX portrait / vertical art pieces in the entire art membership» | https://www.reddit.com/r/TheFrame/comments/1h9eijv/ | 2024-12-08 | 10/12 | новое |
| 14 | Docent (42/21): «Anything that is alternative for better than the under reliable Samsung app is a godsend!» | https://www.reddit.com/r/TheFrame/comments/1tr869b/ | 2026 | 42/21 | канал + |
| 15 | Art Store: «$99 bucks a year, the cost of a cheap lunch» против «hate recurring subscription fees» | https://www.reddit.com/r/TheFrame/comments/1tgfz94/ | 2026 | 14/43 | цены |
| 16 | Ротацию просят: «rotate through 24/7/365»; «картины дня» как запроса нет | https://www.reddit.com/r/TheFrame/comments/197pg0q/ | 2024-01-16 | 3/4 | новое |

Счёт: заполнить/без паспарту — 8 тредов; паспарту или целиком с выбором — 6; голые чёрные поля в положительном свете — ни одного. Бесплатные DIY-инструменты (SAWSUBE, Docent, Samsung TV Art Uploader, Frame Art Manager, NGA cropper, Reframed, Wall Art Frame) почти все стоят на Python-библиотеке NickWaterton для Frame.

### Android
| # | Находка | Тред | Дата | Очки/комм. | Claim |
|---|---|---|---|---|---|
| 1 | Muzei: «can't find any way to have my background picture not zoom in … automatic zoom sometimes reappears» | https://www.reddit.com/r/androidapps/comments/16lpr90/ | 2023-09-22 | 0/0 | C1 + |
| 2 | «[Muzei] doesn't allow centering images» | https://www.reddit.com/r/androidapps/comments/sh2mhz/ | 2022-01-31 | 0/6 | C1 + |
| 3 | «Muzei has been the most recommended but I can't find any sources that's still active … now absolutely nothing» | https://www.reddit.com/r/androidapps/comments/1t7xa1j/ | 2026-05-09 | 8/7 | новое: категория пустеет |
| 4 | «wallpaper app with famous paintings only … not ugly … Bonus if it also tells the story» — без ответов | https://www.reddit.com/r/androidapps/comments/1oxabps/ | 2025-11-14 | 0/0 | спрос есть, отклика нет |
| 5 | Жалоб на размытие/затемнение/качество в Muzei не найдено; r/Muzei почти мёртв (последнее — 2019) | — | 2026-09-29 | — | C1 − (для «размытия») |

### Mac
| # | Находка | Тред | Дата | Очки/комм. | Claim |
|---|---|---|---|---|---|
| 1 | «Why can't I change wallpaper on different spaces? … it's just boring grey» | https://www.reddit.com/r/MacOS/comments/1qy9e5q/ | 2026-02-07 | 1/2 | C2 + |
| 2 | «Wallpaper being reset every time I change desktops … Tahoe 26.3» | https://www.reddit.com/r/MacOS/comments/1r5wa3r/ | 2026-02-16 | 1/3 | C2 + |
| 3 | «Multiple wallpapers with multiple spaces …» | https://www.reddit.com/r/MacOS/comments/1wjl9a2/ | 2026-09-18 | 0/1 | C2 + |
| 4 | «I couldn't find a simple daily wallpaper app for macOS … something more 'set and forget'» | https://www.reddit.com/r/macapps/comments/1r07e71/ | 2026-02-09 | 1/2 | новое: пробел «поставил и забыл» |
| 5 | Открытое приложение ежедневных обоев (Bing, APOD) принято тепло | https://www.reddit.com/r/macapps/comments/1jf3h7i/ | 2025-03-19 | 32/5 | канал + |
| 6 | Жалоб на обрезку картин на Mac не найдено | — | — | — | C1 тишина |

### GNOME и арт-сообщества
- Equinox (GTK4, ежедневные обои Bing/Spotlight/NASA) принят 32/4; в комментариях — Damask, Variety; упомянуто правило Flathub против AI-сделанных продуктов — https://www.reddit.com/r/gnome/comments/1w9szk8/ (2026-09-07).
- Живопись на GNOME никто не просит; r/museum, r/ArtHistory — данных нет (таймауты). Об искусстве на экране спрашивают в сабреддитах устройств (Frame, Meural), не в арт-сабреддитах.

### Канал
- r/TheFrame: бесплатные инструменты от авторов — 37–72 очка; «free tier is huuge» приняли; изредка посты удаляют.
- r/macapps: запуски обоев — Wallper 148/30, gifPaper 98/183, open-source daily wallpaper 32/5; шаблон «What problem it solves / Comparison».
- r/gnome: Equinox 32/4, Wallpaper Engine for GNOME 78/11.
- r/androidapps: слабый канал — мало живых тредов, удалённые посты разработчиков.
- Риск: приложение «как реклама других проектов» — самопродвижение; выигрывали посты, где автор с первой строки говорит, что он автор и что бесплатно.
