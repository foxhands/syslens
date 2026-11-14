//! Interface languages. Texts are written in Russian in the code and
//! translated when shown. Russian, English and Ukrainian live in the tables
//! below; German, Spanish and Polish are separate lists keyed by the English
//! text (lang_de.rs, lang_es.rs, lang_pl.rs) and fall back to English.

use std::sync::atomic::{AtomicU8, Ordering};

#[path = "lang_de.rs"]
mod de;
#[path = "lang_es.rs"]
mod es;
#[path = "lang_pl.rs"]
mod pl;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ru,
    En,
    Uk,
    De,
    Es,
    Pl,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::Ru, Lang::En, Lang::Uk, Lang::De, Lang::Es, Lang::Pl];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
            Lang::Uk => "uk",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::Pl => "pl",
        }
    }
    /// Short label for the language button.
    pub fn short(self) -> &'static str {
        match self {
            Lang::Ru => "RU",
            Lang::En => "EN",
            Lang::Uk => "UA",
            Lang::De => "DE",
            Lang::Es => "ES",
            Lang::Pl => "PL",
        }
    }
    /// The language's own name.
    pub fn native(self) -> &'static str {
        match self {
            Lang::Ru => "Русский",
            Lang::En => "English",
            Lang::Uk => "Українська",
            Lang::De => "Deutsch",
            Lang::Es => "Español",
            Lang::Pl => "Polski",
        }
    }
    pub fn from_code(c: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == c.trim())
    }
    /// The list of a language that is kept apart from the main table.
    fn extra(self) -> Option<&'static [(&'static str, &'static str)]> {
        match self {
            Lang::De => Some(de::TEXTS),
            Lang::Es => Some(es::TEXTS),
            Lang::Pl => Some(pl::TEXTS),
            _ => None,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) {
    CURRENT.store(l as u8, Ordering::Relaxed);
}

pub fn get() -> Lang {
    let i = CURRENT.load(Ordering::Relaxed) as usize;
    Lang::ALL.get(i).copied().unwrap_or(Lang::Ru)
}

/// The system's display language; English when it is not one of ours.
pub fn system() -> Lang {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        // SAFETY: plain Win32 call without arguments.
        let id = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
        return match id {
            0x19 => Lang::Ru,
            0x22 => Lang::Uk,
            0x07 => Lang::De,
            0x0a => Lang::Es,
            0x15 => Lang::Pl,
            _ => Lang::En,
        };
    }
    #[allow(unreachable_code)]
    {
        // Linux and others: LC_ALL, LC_MESSAGES or LANG ("uk_UA.UTF-8").
        let v = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        from_locale(&v)
    }
}

/// "ru_RU.UTF-8" → Russian, "de_AT" → German, anything unknown → English.
pub fn from_locale(v: &str) -> Lang {
    v.get(..2).and_then(Lang::from_code).unwrap_or(Lang::En)
}

type Row = (&'static str, &'static str, &'static str);

/// Russian → English, Ukrainian.
const TABLE: &[Row] = &[
    // pages and menu
    ("Компьютер", "Computer", "Комп'ютер"),
    ("Суммарная информация", "Summary", "Зведена інформація"),
    ("Системная плата", "Motherboard", "Системна плата"),
    ("ЦП", "CPU", "ЦП"),
    ("Память", "Memory", "Пам'ять"),
    ("Операционная система", "Operating system", "Операційна система"),
    ("Отображение", "Display", "Відображення"),
    ("Видеоадаптер", "Video adapter", "Відеоадаптер"),
    ("Монитор", "Monitor", "Монітор"),
    ("Мультимедиа", "Multimedia", "Мультимедіа"),
    ("Звуковые устройства", "Audio devices", "Звукові пристрої"),
    ("Хранение данных", "Storage", "Зберігання даних"),
    ("Физические диски", "Physical disks", "Фізичні диски"),
    ("Логические диски", "Logical disks", "Логічні диски"),
    ("Сеть", "Network", "Мережа"),
    ("Сетевые адаптеры", "Network adapters", "Мережеві адаптери"),
    ("Программы", "Programs", "Програми"),
    ("Установленные программы", "Installed programs", "Встановлені програми"),
    // window, buttons, messages
    ("SysLens — сведения о системе", "SysLens — system information", "SysLens — відомості про систему"),
    ("Сведения о системе", "System information", "Відомості про систему"),
    ("Сбор сведений о системе…", "Collecting system information…", "Збір відомостей про систему…"),
    ("Сведения обновлены", "Information updated", "Відомості оновлено"),
    ("Сохранить отчёт", "Save report", "Зберегти звіт"),
    ("Копировать", "Copy", "Копіювати"),
    ("Обновить", "Refresh", "Оновити"),
    ("Страница скопирована в буфер обмена", "Page copied to clipboard", "Сторінку скопійовано в буфер обміну"),
    ("Отчёт о системе (SysLens)", "System report (SysLens)", "Звіт про систему (SysLens)"),
    ("Отчёт сохранён:", "Report saved:", "Звіт збережено:"),
    ("Не удалось сохранить отчёт:", "Could not save the report:", "Не вдалося зберегти звіт:"),
    ("Не удалось запустить PowerShell:", "Could not start PowerShell:", "Не вдалося запустити PowerShell:"),
    ("Пустой ответ системы", "Empty response from the system", "Порожня відповідь системи"),
    ("Система не ответила вовремя", "The system did not respond in time", "Система не відповіла вчасно"),
    ("Повторить", "Try again", "Повторити"),
    // new pages
    ("Нагрузка сейчас", "Load now", "Навантаження зараз"),
    ("Батарея", "Battery", "Батарея"),
    ("Обновления системы", "System updates", "Оновлення системи"),
    ("Устройства", "Devices", "Пристрої"),
    ("Драйверы", "Drivers", "Драйвери"),
    ("Устройства USB", "USB devices", "Пристрої USB"),
    ("Принтеры", "Printers", "Принтери"),
    ("Автозагрузка", "Startup", "Автозавантаження"),
    ("Службы", "Services", "Служби"),
    ("Службы с автозапуском", "Services that start automatically", "Служби з автозапуском"),
    ("Драйверы сторонних производителей", "Third-party drivers", "Драйвери сторонніх виробників"),
    ("Работает", "Running", "Працює"),
    ("Остановлена", "Stopped", "Зупинена"),
    // live load
    ("Оперативная память", "Memory in use", "Оперативна пам'ять"),
    ("Файл подкачки", "Page file", "Файл підкачки"),
    ("Диски", "Disks", "Диски"),
    ("Загрузка по ядрам", "Load by core", "Навантаження по ядрах"),
    // battery and disk health
    ("Заряд", "Charge", "Заряд"),
    ("Питание", "Power", "Живлення"),
    ("От сети", "Plugged in", "Від мережі"),
    ("Разряжается", "Discharging", "Розряджається"),
    ("Заряжается", "Charging", "Заряджається"),
    ("Заряжена", "Fully charged", "Заряджена"),
    ("Осталось времени", "Time left", "Залишилося часу"),
    ("Полная ёмкость", "Full capacity", "Повна ємність"),
    ("Заводская ёмкость", "Design capacity", "Заводська ємність"),
    ("Износ", "Wear", "Знос"),
    ("Циклов заряда", "Charge cycles", "Циклів заряду"),
    ("Батарея не обнаружена", "No battery found", "Батарею не виявлено"),
    ("Температура", "Temperature", "Температура"),
    ("Наработка", "Power-on time", "Напрацювання"),
    ("Ошибки чтения", "Read errors", "Помилки читання"),
    ("Ошибки записи", "Write errors", "Помилки запису"),
    ("Подробности", "Details", "Подробиці"),
    (
        "Температура и износ видны при запуске от имени администратора",
        "Temperature and wear are shown when the program is run as administrator",
        "Температуру та знос видно під час запуску від імені адміністратора",
    ),
    // search, copy, sorting
    ("Поиск по всем страницам…", "Search all pages…", "Пошук по всіх сторінках…"),
    ("Значение скопировано", "Value copied", "Значення скопійовано"),
    ("Сортировка:", "Sort:", "Сортування:"),
    ("по имени", "by name", "за назвою"),
    ("по размеру", "by size", "за розміром"),
    ("по дате", "by date", "за датою"),
    ("по издателю", "by publisher", "за видавцем"),
    ("SysLens-отчёт", "SysLens-report", "SysLens-звіт"),
    ("Да", "Yes", "Так"),
    ("Нет", "No", "Ні"),
    ("Ошибка разбора данных:", "Data parsing error:", "Помилка розбору даних:"),
    (
        "Личные данные скрыты (имя ПК, пользователь, серийные номера, адреса). Нажмите, чтобы показать.",
        "Personal data is hidden (PC name, user, serial numbers, addresses). Click to show it.",
        "Особисті дані приховано (ім'я ПК, користувач, серійні номери, адреси). Натисніть, щоб показати.",
    ),
    (
        "Скрыть личные данные — удобно перед тем, как делиться снимком или отчётом",
        "Hide personal data — handy before sharing a screenshot or report",
        "Приховати особисті дані — зручно перед тим, як ділитися знімком або звітом",
    ),
    ("О программе", "About", "Про програму"),
    ("Светлая тема", "Light theme", "Світла тема"),
    ("Тёмная тема", "Dark theme", "Темна тема"),
    ("Язык", "Language", "Мова"),
    ("Очистить поиск", "Clear search", "Очистити пошук"),
    ("Ничего не найдено", "Nothing found", "Нічого не знайдено"),
    (
        "Сведения о процессоре, системной плате, памяти,",
        "Information about the processor, motherboard, memory,",
        "Відомості про процесор, системну плату, пам'ять,",
    ),
    (
        "видеоадаптере, дисках и сети этого компьютера.",
        "graphics, disks and network of this computer.",
        "відеоадаптер, диски та мережу цього комп'ютера.",
    ),
    ("F5 — обновить сведения", "F5 — refresh", "F5 — оновити відомості"),
    ("скрыто", "hidden", "приховано"),
    ("Процессор", "Processor", "Процесор"),
    ("Видеокарта", "Graphics card", "Відеокарта"),
    ("Диск", "Disk", "Диск"),
    // values
    ("Включено", "Enabled", "Увімкнено"),
    ("Отключено", "Disabled", "Вимкнено"),
    ("Неизвестно", "Unknown", "Невідомо"),
    ("Настольный компьютер", "Desktop computer", "Настільний комп'ютер"),
    ("Мобильный компьютер", "Mobile computer", "Мобільний комп'ютер"),
    ("Рабочая станция", "Workstation", "Робоча станція"),
    ("Сервер", "Server", "Сервер"),
    ("Планшет", "Tablet", "Планшет"),
    ("Съёмный диск", "Removable disk", "Знімний диск"),
    ("Локальный диск", "Local disk", "Локальний диск"),
    // rows and sections
    ("Тип компьютера", "Computer type", "Тип комп'ютера"),
    ("Сборка ОС", "OS build", "Збірка ОС"),
    ("Имя компьютера", "Computer name", "Ім'я комп'ютера"),
    ("Имя пользователя", "User name", "Ім'я користувача"),
    ("Дата и время", "Date and time", "Дата і час"),
    ("Время работы", "Uptime", "Час роботи"),
    ("Тип ЦП", "CPU type", "Тип ЦП"),
    ("Системная память", "System memory", "Системна пам'ять"),
    ("Тип BIOS", "BIOS type", "Тип BIOS"),
    ("Звуковой адаптер", "Audio adapter", "Звуковий адаптер"),
    ("Дисковый накопитель", "Disk drive", "Дисковий накопичувач"),
    ("Разделы", "Partitions", "Розділи"),
    ("Общий размер", "Total size", "Загальний розмір"),
    ("Первичный адрес IP", "Primary IP address", "Основна IP-адреса"),
    ("Сетевой адаптер", "Network adapter", "Мережевий адаптер"),
    ("Свойства ЦП", "CPU properties", "Властивості ЦП"),
    ("Производитель", "Manufacturer", "Виробник"),
    ("Описание", "Description", "Опис"),
    ("Разъём", "Socket", "Роз'єм"),
    ("Ядра", "Cores", "Ядра"),
    ("Логические процессоры", "Logical processors", "Логічні процесори"),
    ("Исходная частота", "Base clock", "Базова частота"),
    ("Текущая частота", "Current clock", "Поточна частота"),
    ("Разрядность", "Address width", "Розрядність"),
    ("ID процессора", "Processor ID", "ID процесора"),
    ("Виртуализация", "Virtualization", "Віртуалізація"),
    ("Кэш", "Cache", "Кеш"),
    ("Кэш L1", "L1 cache", "Кеш L1"),
    ("Кэш L2", "L2 cache", "Кеш L2"),
    ("Кэш L3", "L3 cache", "Кеш L3"),
    ("Свойства системной платы", "Motherboard properties", "Властивості системної плати"),
    ("Версия", "Version", "Версія"),
    ("Серийный номер", "Serial number", "Серійний номер"),
    ("Модель", "Model", "Модель"),
    ("Тип системы", "System type", "Тип системи"),
    ("Число процессоров", "Processors", "Кількість процесорів"),
    ("Свойства шины памяти", "Memory bus properties", "Властивості шини пам'яті"),
    ("Разъёмы памяти", "Memory slots", "Роз'єми пам'яті"),
    ("Занято разъёмов", "Slots used", "Зайнято роз'ємів"),
    ("Макс. объём памяти", "Max. memory", "Макс. обсяг пам'яті"),
    ("Тип памяти", "Memory type", "Тип пам'яті"),
    ("Скорость памяти", "Memory speed", "Швидкість пам'яті"),
    ("Физическая память", "Physical memory", "Фізична пам'ять"),
    ("Всего", "Total", "Усього"),
    ("Занято", "Used", "Зайнято"),
    ("Свободно", "Free", "Вільно"),
    ("Загрузка", "Load", "Завантаження"),
    ("Виртуальная память", "Virtual memory", "Віртуальна пам'ять"),
    ("Модуль памяти", "Memory module", "Модуль пам'яті"),
    ("Размер модуля", "Module size", "Розмір модуля"),
    ("Тип модуля", "Module type", "Тип модуля"),
    ("Текущая скорость", "Current speed", "Поточна швидкість"),
    ("Напряжение", "Voltage", "Напруга"),
    ("Ширина шины", "Bus width", "Ширина шини"),
    ("Банк", "Bank", "Банк"),
    ("Статус", "Status", "Статус"),
    ("Нет данных о модулях памяти", "No memory module data", "Немає даних про модулі пам'яті"),
    ("Свойства BIOS", "BIOS properties", "Властивості BIOS"),
    ("Версия BIOS", "BIOS version", "Версія BIOS"),
    ("Дата BIOS", "BIOS date", "Дата BIOS"),
    ("Версия SMBIOS", "SMBIOS version", "Версія SMBIOS"),
    ("Свойства операционной системы", "Operating system properties", "Властивості операційної системи"),
    ("Название ОС", "OS name", "Назва ОС"),
    ("Версия ОС", "OS version", "Версія ОС"),
    ("Дата установки", "Install date", "Дата встановлення"),
    ("Зарегистрированный пользователь", "Registered user", "Зареєстрований користувач"),
    ("Папка Windows", "Windows folder", "Папка Windows"),
    ("Системная папка", "System folder", "Системна папка"),
    ("Текущий сеанс", "Current session", "Поточний сеанс"),
    ("Пользователь", "User", "Користувач"),
    ("Домен / рабочая группа", "Domain / workgroup", "Домен / робоча група"),
    ("Последняя загрузка", "Last boot", "Останнє завантаження"),
    ("Свойства видеоадаптера", "Video adapter properties", "Властивості відеоадаптера"),
    ("Графический процессор", "GPU", "Графічний процесор"),
    ("Объём видеопамяти", "Video memory", "Обсяг відеопам'яті"),
    ("Тип ЦАП", "DAC type", "Тип ЦАП"),
    ("ID устройства", "Device ID", "ID пристрою"),
    ("Драйвер", "Driver", "Драйвер"),
    ("Версия драйвера", "Driver version", "Версія драйвера"),
    ("Дата драйвера", "Driver date", "Дата драйвера"),
    ("Текущий видеорежим", "Current video mode", "Поточний відеорежим"),
    ("Разрешение", "Resolution", "Роздільна здатність"),
    ("Глубина цвета", "Color depth", "Глибина кольору"),
    ("Частота обновления", "Refresh rate", "Частота оновлення"),
    ("Свойства монитора", "Monitor properties", "Властивості монітора"),
    ("Название", "Name", "Назва"),
    ("Производитель (код)", "Manufacturer (code)", "Виробник (код)"),
    ("Год выпуска", "Year of manufacture", "Рік випуску"),
    ("Нет данных", "No data", "Немає даних"),
    ("Физический диск", "Physical disk", "Фізичний диск"),
    ("Тип носителя", "Media type", "Тип носія"),
    ("Интерфейс", "Interface", "Інтерфейс"),
    ("Состояние", "Health", "Стан"),
    ("Версия прошивки", "Firmware version", "Версія прошивки"),
    ("Метка тома", "Volume label", "Мітка тому"),
    ("Тип", "Type", "Тип"),
    ("Файловая система", "File system", "Файлова система"),
    ("Свободно %", "Free %", "Вільно %"),
    ("MAC-адрес", "MAC address", "MAC-адреса"),
    ("Скорость", "Speed", "Швидкість"),
    ("Подключён", "Connected", "Підключено"),
    ("Адреса", "Addresses", "Адреси"),
    ("IP-адреса", "IP addresses", "IP-адреси"),
    ("Шлюз", "Gateway", "Шлюз"),
    // advice page
    ("Оценки и подсказки", "Advice", "Оцінки та підказки"),
    ("Узкие места", "Bottlenecks", "Вузькі місця"),
    ("Что можно улучшить", "What can be improved", "Що можна покращити"),
    ("Давно не обновлялось", "Not updated for a long time", "Давно не оновлювалося"),
    ("Готовность к Windows 11", "Windows 11 readiness", "Готовність до Windows 11"),
    ("Драйверы старше 5 лет", "Drivers older than 5 years", "Драйвери, старші за 5 років"),
    (
        "Программы, установленные более 3 лет назад",
        "Programs installed more than 3 years ago",
        "Програми, встановлені понад 3 роки тому",
    ),
    ("Перезагрузка", "Restart", "Перезавантаження"),
    ("Итог", "Verdict", "Підсумок"),
    ("Слоты памяти", "Memory slots free", "Слоти пам'яті"),
    ("Максимум памяти", "Memory limit", "Максимум пам'яті"),
    ("Обновления Windows", "Windows updates", "Оновлення Windows"),
    (
        "Память занята на {0} % — её не хватает для того, что сейчас запущено.",
        "Memory is {0} % full — there is not enough of it for what is running now.",
        "Пам'ять зайнята на {0} % — її не вистачає для того, що зараз запущено.",
    ),
    (
        "Всего {0} памяти — для современных программ этого мало.",
        "Only {0} of memory — too little for today's programs.",
        "Лише {0} пам'яті — для сучасних програм цього мало.",
    ),
    (
        "Один модуль памяти вместо двух: память работает в одноканальном режиме, а это медленнее.",
        "One memory module instead of two: memory works in single-channel mode, which is slower.",
        "Один модуль пам'яті замість двох: пам'ять працює в одноканальному режимі, а це повільніше.",
    ),
    (
        "Осталось {0} свободного места из {1} — диск почти полон.",
        "{0} free of {1} — the disk is almost full.",
        "Залишилося {0} вільного місця з {1} — диск майже заповнений.",
    ),
    (
        "Нет ни одного SSD: система работает с жёсткого диска. С SSD компьютер загружается и работает заметно быстрее.",
        "No SSD at all: the system runs from a hard disk. With an SSD the computer starts and works noticeably faster.",
        "Немає жодного SSD: система працює з жорсткого диска. З SSD комп'ютер завантажується і працює помітно швидше.",
    ),
    (
        "Состояние «{0}» — сделайте резервную копию важных файлов.",
        "Health is “{0}” — back up your important files.",
        "Стан «{0}» — зробіть резервну копію важливих файлів.",
    ),
    (
        "Износ {0} % — диск близок к концу срока службы.",
        "Wear {0} % — the disk is near the end of its life.",
        "Знос {0} % — диск близький до кінця терміну служби.",
    ),
    (
        "Износ батареи {0} % — она держит заряд заметно меньше, чем новая.",
        "Battery wear {0} % — it holds noticeably less charge than a new one.",
        "Знос батареї {0} % — вона тримає заряд помітно менше, ніж нова.",
    ),
    (
        "Компьютер работает без перезагрузки {0} дн. — перезагрузка часто убирает подтормаживания.",
        "The computer has run for {0} days without a restart — a restart often cures slowness.",
        "Комп'ютер працює без перезавантаження {0} дн. — перезавантаження часто прибирає пригальмовування.",
    ),
    ("Явных узких мест не видно.", "No obvious bottlenecks.", "Явних вузьких місць не видно."),
    ("Свободно слотов памяти: {0} из {1}.", "Free memory slots: {0} of {1}.", "Вільних слотів пам'яті: {0} з {1}."),
    (
        "Все слоты памяти заняты: чтобы добавить память, модули придётся заменить на более ёмкие.",
        "All memory slots are taken: to add memory, the modules have to be replaced with larger ones.",
        "Усі слоти пам'яті зайняті: щоб додати пам'ять, модулі доведеться замінити на місткіші.",
    ),
    (
        "Плата поддерживает до {0} памяти, сейчас установлено {1}.",
        "The board supports up to {0} of memory, {1} is installed now.",
        "Плата підтримує до {0} пам'яті, зараз встановлено {1}.",
    ),
    (
        "Установлен максимум памяти, который поддерживает плата.",
        "The most memory the board supports is already installed.",
        "Встановлено максимум пам'яті, який підтримує плата.",
    ),
    ("Сведений о слотах памяти нет.", "No information about memory slots.", "Відомостей про слоти пам'яті немає."),
    (
        "BIOS от {0} — ему больше {1} лет. Новая версия может быть на сайте производителя платы.",
        "The BIOS is from {0} — more than {1} years old. A newer version may be on the board maker's site.",
        "BIOS від {0} — йому понад {1} років. Нова версія може бути на сайті виробника плати.",
    ),
    (
        "Драйвер от {0} — ему больше года. Новая версия может быть на сайте производителя видеокарты.",
        "The driver is from {0} — more than a year old. A newer version may be on the graphics card maker's site.",
        "Драйвер від {0} — йому понад рік. Нова версія може бути на сайті виробника відеокарти.",
    ),
    (
        "Последнее обновление Windows установлено {0} — больше {1} дн. назад. Проверьте Центр обновления.",
        "The last Windows update was installed on {0} — more than {1} days ago. Check Windows Update.",
        "Останнє оновлення Windows встановлено {0} — понад {1} дн. тому. Перевірте Центр оновлення.",
    ),
    ("Ничего давнего не найдено.", "Nothing old found.", "Нічого давнього не знайдено."),
    ("драйвер от {0}", "driver from {0}", "драйвер від {0}"),
    ("установлена {0}", "installed on {0}", "встановлено {0}"),
    ("и ещё {0}", "and {0} more", "і ще {0}"),
    (
        "Процессор: 64 бита, от 2 ядер, от 1 ГГц",
        "Processor: 64-bit, 2+ cores, 1+ GHz",
        "Процесор: 64 біти, від 2 ядер, від 1 ГГц",
    ),
    ("Память: от 4 ГБ", "Memory: 4 GB or more", "Пам'ять: від 4 ГБ"),
    ("Системный диск: от 64 ГБ", "System disk: 64 GB or more", "Системний диск: від 64 ГБ"),
    ("Загрузка через UEFI", "UEFI boot", "Завантаження через UEFI"),
    ("Модуль TPM 2.0", "TPM 2.0 chip", "Модуль TPM 2.0"),
    (
        "Место для обновления: от 25 ГБ свободно",
        "Room for the update: 25 GB free or more",
        "Місце для оновлення: від 25 ГБ вільно",
    ),
    ("подходит", "suitable", "підходить"),
    ("не подходит", "not suitable", "не підходить"),
    ("включён", "on", "увімкнено"),
    ("выключен или не поддерживается", "off or not supported", "вимкнено або не підтримується"),
    ("не найден", "not found", "не знайдено"),
    ("не удалось определить", "could not be determined", "не вдалося визначити"),
    (
        "На этом компьютере уже стоит Windows 11.",
        "This computer already runs Windows 11.",
        "На цьому комп'ютері вже встановлено Windows 11.",
    ),
    (
        "Основные требования Windows 11 выполнены. Модель процессора со списком Microsoft не сверялась.",
        "The basic Windows 11 requirements are met. The processor model was not compared with Microsoft's list.",
        "Основні вимоги Windows 11 виконано. Модель процесора зі списком Microsoft не звірялася.",
    ),
    (
        "Часть требований Windows 11 проверить не удалось (строки с «?»), остальные выполнены.",
        "Some Windows 11 requirements could not be checked (rows marked “?”); the rest are met.",
        "Частину вимог Windows 11 перевірити не вдалося (рядки з «?»), решту виконано.",
    ),
    (
        "Компьютер не подходит для Windows 11: выполнены не все требования.",
        "This computer is not ready for Windows 11: not all requirements are met.",
        "Комп'ютер не підходить для Windows 11: виконано не всі вимоги.",
    ),
    // comparison of two computers
    ("Сравнение", "Comparison", "Порівняння"),
    ("Сравнение с отчётом:", "Compared with the report:", "Порівняння зі звітом:"),
    (
        "«Другой» — компьютер из отчёта, «этот» — компьютер, на котором запущена программа. Имена, серийные номера и адреса не сравниваются.",
        "“Other” is the computer from the report, “this one” is the computer the program runs on. Names, serial numbers and addresses are not compared.",
        "«Інший» — комп'ютер зі звіту, «цей» — комп'ютер, на якому запущено програму. Імена, серійні номери та адреси не порівнюються.",
    ),
    (
        "Сохраните на другом компьютере отчёт в формате JSON («Сохранить отчёт» → JSON), перенесите файл сюда и откройте его — здесь будут видны различия в железе и программах.",
        "On the other computer save a report as JSON (“Save report” → JSON), bring the file here and open it — this page will show how the hardware and programs differ.",
        "Збережіть на іншому комп'ютері звіт у форматі JSON («Зберегти звіт» → JSON), перенесіть файл сюди й відкрийте його — тут буде видно відмінності в залізі та програмах.",
    ),
    ("Открыть отчёт другого компьютера", "Open another computer's report", "Відкрити звіт іншого комп'ютера"),
    ("Открыть другой отчёт", "Open another report", "Відкрити інший звіт"),
    ("Закрыть сравнение", "Close the comparison", "Закрити порівняння"),
    ("Различий нет", "No differences", "Відмінностей немає"),
    ("Отчёт другого компьютера открыт", "The other computer's report is open", "Звіт іншого комп'ютера відкрито"),
    ("Не удалось открыть файл:", "Could not open the file:", "Не вдалося відкрити файл:"),
    ("Это не отчёт SysLens в формате JSON", "This is not a SysLens report in JSON format", "Це не звіт SysLens у форматі JSON"),
    (
        "Этот отчёт сделан старой версией SysLens — сохраните его заново в новой",
        "This report was made by an old version of SysLens — save it again with the new one",
        "Цей звіт зроблено старою версією SysLens — збережіть його заново в новій",
    ),
    (
        "Окно выбора файла недоступно. Запустите программу так: syslens --compare отчёт.json",
        "No file window is available. Start the program like this: syslens --compare report.json",
        "Вікно вибору файлу недоступне. Запустіть програму так: syslens --compare звіт.json",
    ),
    ("только на другом", "only on the other", "лише на іншому"),
    ("только на этом", "only on this one", "лише на цьому"),
    ("другой", "other", "інший"),
    ("этот", "this one", "цей"),
    // favourites
    ("Избранное", "Pinned", "Вибране"),
    ("Закрепить на сводной странице", "Pin to the summary page", "Закріпити на зведеній сторінці"),
    ("Открепить от сводной страницы", "Unpin from the summary page", "Відкріпити від зведеної сторінки"),
    ("Строка закреплена на сводной странице", "The row is pinned to the summary page", "Рядок закріплено на зведеній сторінці"),
    ("Строка откреплена", "The row is unpinned", "Рядок відкріплено"),
    (
        "Важные строки можно закрепить здесь: щёлкните правой кнопкой по значению на любой странице.",
        "Important rows can be pinned here: right-click a value on any page.",
        "Важливі рядки можна закріпити тут: клацніть правою кнопкою по значенню на будь-якій сторінці.",
    ),
    // picture, printing, short summary
    (
        "Сохранить страницу как картинку — личные данные на ней будут скрыты",
        "Save the page as a picture — personal data will be hidden on it",
        "Зберегти сторінку як картинку — особисті дані на ній буде приховано",
    ),
    ("Сохранить страницу как картинку", "Save the page as a picture", "Зберегти сторінку як картинку"),
    ("Картинка сохранена:", "Picture saved:", "Картинку збережено:"),
    ("Не удалось сохранить картинку", "Could not save the picture", "Не вдалося зберегти картинку"),
    ("Печать или сохранение в PDF", "Print or save as PDF", "Друк або збереження в PDF"),
    (
        "Отчёт открыт в браузере: выберите принтер или «Сохранить как PDF»",
        "The report is open in the browser: choose a printer or “Save as PDF”",
        "Звіт відкрито в браузері: виберіть принтер або «Зберегти як PDF»",
    ),
    ("Не удалось открыть браузер", "Could not open the browser", "Не вдалося відкрити браузер"),
    ("Эту страницу", "This page", "Цю сторінку"),
    ("Короткую сводку одной строкой", "A one-line summary", "Коротке зведення одним рядком"),
    (
        "Короткая сводка скопирована в буфер обмена",
        "The one-line summary is copied to the clipboard",
        "Коротке зведення скопійовано в буфер обміну",
    ),
    // settings and updates
    ("Настройки", "Settings", "Налаштування"),
    ("Размеры дисков", "Disk sizes", "Розміри дисків"),
    ("как в Windows: 1 ГБ = 1024 МБ", "as in Windows: 1 GB = 1024 MB", "як у Windows: 1 ГБ = 1024 МБ"),
    ("как на коробке: 1 ГБ = 1000 МБ", "as on the box: 1 GB = 1000 MB", "як на коробці: 1 ГБ = 1000 МБ"),
    ("Значок возле часов с нагрузкой", "Load icon next to the clock", "Значок біля годинника з навантаженням"),
    ("Проверить обновления", "Check for updates", "Перевірити оновлення"),
    ("Проверяю, нет ли новой версии…", "Checking for a new version…", "Перевіряю, чи немає нової версії…"),
    ("У вас последняя версия.", "You have the latest version.", "У вас остання версія."),
    ("Вышла новая версия:", "A new version is out:", "Вийшла нова версія:"),
    ("Скачать новую версию", "Download the new version", "Завантажити нову версію"),
    (
        "Не удалось проверить: нет связи с GitHub.",
        "Could not check: no connection to GitHub.",
        "Не вдалося перевірити: немає зв'язку з GitHub.",
    ),
    ("Автор:", "Author:", "Автор:"),
];

/// Units and words inside values.
const WORDS: &[Row] = &[
    ("МБ", "MB", "МБ"),
    ("ГБ", "GB", "ГБ"),
    ("КБ", "KB", "КБ"),
    ("ТБ", "TB", "ТБ"),
    ("МГц", "MHz", "МГц"),
    ("ГГц", "GHz", "ГГц"),
    ("КБ/с", "KB/s", "КБ/с"),
    ("МБ/с", "MB/s", "МБ/с"),
    ("ГБ/с", "GB/s", "ГБ/с"),
    ("ТБ/с", "TB/s", "ТБ/с"),
    ("мВт·ч", "mWh", "мВт·год"),
    ("мА·ч", "mAh", "мА·год"),
    ("ч", "h", "год"),
    ("мин", "min", "хв"),
    ("основной", "default", "основний"),
    ("из", "of", "з"),
    ("Гц", "Hz", "Гц"),
    ("бит", "bit", "біт"),
    ("Мбит/с", "Mbit/s", "Мбіт/с"),
    ("свободно", "free", "вільно"),
    ("версия", "version", "версія"),
    ("Неизвестно", "Unknown", "Невідомо"),
    ("Майкрософт", "Microsoft", "Microsoft"),
];

/// The text of a table row in language `l`.
fn pick(r: &Row, l: Lang) -> &'static str {
    match l {
        Lang::Ru => r.0,
        Lang::En => r.1,
        Lang::Uk => r.2,
        other => other
            .extra()
            .and_then(|list| list.iter().find(|x| x.0 == r.1))
            .map(|x| x.1)
            .unwrap_or(r.1),
    }
}

fn lookup(table: &'static [Row], s: &str, l: Lang) -> Option<&'static str> {
    table.iter().find(|r| r.0 == s).map(|r| pick(r, l))
}

/// A fixed text in the current language.
pub fn t(s: &str) -> String {
    tr(s, get())
}

/// A text in language `l`: exact match first, then a known beginning
/// ("Модуль памяти  DIMM1", "Отчёт сохранён: …") with the rest kept.
pub fn tr(s: &str, l: Lang) -> String {
    if l == Lang::Ru {
        return s.to_string();
    }
    if let Some(x) = lookup(TABLE, s, l) {
        return x.to_string();
    }
    let mut best: Option<&Row> = None;
    for r in TABLE {
        if s.len() > r.0.len() && s.starts_with(r.0) {
            let next = s[r.0.len()..].chars().next().unwrap_or(' ');
            if !next.is_alphanumeric() && best.map_or(true, |b| r.0.len() > b.0.len()) {
                best = Some(r);
            }
        }
    }
    match best {
        Some(r) => format!("{}{}", pick(r, l), &s[r.0.len()..]),
        None => s.to_string(),
    }
}

/// A sentence with places for values: "Свободно слотов памяти: {0} из {1}."
/// The values are translated like row values (units and the like).
pub fn fmt(template: &str, args: &[&str], l: Lang) -> String {
    let mut out = tr(template, l);
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &value("", a, l));
    }
    out
}

fn word(w: &str, l: Lang) -> String {
    // keep surrounding punctuation: "(466", "ГБ,", "свободно)"
    let start = w.find(|c: char| c.is_alphanumeric()).unwrap_or(w.len());
    let end = w.rfind(|c: char| c.is_alphanumeric() || c == '/').map(|i| i + w[i..].chars().next().unwrap().len_utf8()).unwrap_or(start);
    if start >= end {
        return w.to_string();
    }
    let core = &w[start..end];
    match lookup(WORDS, core, l) {
        Some(x) => format!("{}{}{}", &w[..start], x, &w[end..]),
        None => w.to_string(),
    }
}

/// A value in language `l`; `key` is the row's Russian name.
pub fn value(key: &str, v: &str, l: Lang) -> String {
    if l == Lang::Ru {
        return v.to_string();
    }
    if let Some(x) = lookup(TABLE, v, l) {
        return x.to_string();
    }
    // The comparison page: "только на другом: X", "другой: A  →  этот: B".
    for only in ["только на другом", "только на этом"] {
        if let Some(rest) = v.strip_prefix(only).and_then(|r| r.strip_prefix(": ")) {
            return format!("{}: {}", tr(only, l), value(key, rest, l));
        }
    }
    if let Some((a, b)) = v.strip_prefix("другой: ").and_then(|r| r.split_once("  →  этот: ")) {
        return format!("{}: {}  →  {}: {}", tr("другой", l), value(key, a, l), tr("этот", l), value(key, b, l));
    }
    let mut out: Vec<String> = v.split(' ').map(|w| word(w, l)).collect();
    if key == "Время работы" {
        // days and seconds are too short to translate anywhere but here
        let (d, s) = if l == Lang::Uk { ("дн.", "с") } else { ("d", "s") };
        for w in out.iter_mut() {
            match w.as_str() {
                "дн." => *w = d.into(),
                "с" => *w = s.into(),
                _ => {}
            }
        }
    }
    if key == "Напряжение" && l != Lang::Uk {
        if let Some(last) = out.last_mut() {
            if last == "В" {
                *last = "V".into();
            }
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translate() {
        assert_eq!(tr("Модуль памяти  DIMM1", Lang::En), "Memory module  DIMM1");
        assert_eq!(tr("Тип ЦП", Lang::Uk), "Тип ЦП");
        assert_eq!(tr("Установленные программы (63)", Lang::En), "Installed programs (63)");
        assert_eq!(value("x", "150.3 ГБ (14.8 ГБ свободно)", Lang::En), "150.3 GB (14.8 GB free)");
        assert_eq!(value("Время работы", "6 ч 41 мин 35 с", Lang::En), "6 h 41 min 35 s");
        assert_eq!(value("Время работы", "2 дн. 6 ч 41 мин 35 с", Lang::Uk), "2 дн. 6 год 41 хв 35 с");
        assert_eq!(value("x", "CT500P3SSD8  (466 ГБ, NVMe)", Lang::En), "CT500P3SSD8  (466 GB, NVMe)");
        assert_eq!(tr("Сборка ОС", Lang::Ru), "Сборка ОС");
        assert_eq!(value("x", "свободно 14.8 ГБ из 150.3 ГБ", Lang::En), "free 14.8 GB of 150.3 GB");
        assert_eq!(value("x", "Ryzen 5  ·  6C/12T  ·  3.7 ГГц", Lang::Uk), "Ryzen 5  ·  6C/12T  ·  3.7 ГГц");
        assert_eq!(value("x", "↓ 1.2 МБ/с    ↑ 300 КБ/с", Lang::En), "↓ 1.2 MB/s    ↑ 300 KB/s");
    }

    #[test]
    fn comparison_phrases() {
        assert_eq!(
            value("x", "другой: версия 1.0  →  этот: версия 1.1", Lang::En),
            "other: version 1.0  →  this one: version 1.1"
        );
        assert_eq!(value("x", "только на другом: 2 ч 5 мин", Lang::En), "only on the other: 2 h 5 min");
        assert_eq!(value("x", "только на этом: версия 3", Lang::Uk), "лише на цьому: версія 3");
    }

    #[test]
    fn sentences_with_values() {
        assert_eq!(fmt("Свободно слотов памяти: {0} из {1}.", &["2", "4"], Lang::En), "Free memory slots: 2 of 4.");
        assert_eq!(
            fmt("Плата поддерживает до {0} памяти, сейчас установлено {1}.", &["128 ГБ", "16 ГБ"], Lang::En),
            "The board supports up to 128 GB of memory, 16 GB is installed now."
        );
        assert_eq!(fmt("Свободно слотов памяти: {0} из {1}.", &["2", "4"], Lang::Ru), "Свободно слотов памяти: 2 из 4.");
    }

    #[test]
    fn other_languages() {
        assert_eq!(tr("Память", Lang::De), "Arbeitsspeicher");
        assert_eq!(tr("Установленные программы (63)", Lang::Es), "Programas instalados (63)");
        assert_eq!(tr("Сохранить отчёт", Lang::Pl), "Zapisz raport");
        assert_eq!(value("x", "свободно 14.8 ГБ из 150.3 ГБ", Lang::De), "frei 14.8 GB von 150.3 GB");
        assert_eq!(from_locale("de_AT.UTF-8"), Lang::De);
        assert_eq!(from_locale("ja_JP"), Lang::En);
        assert_eq!(Lang::from_code("pl"), Some(Lang::Pl));
    }

    /// Every text has a German, Spanish and Polish form, and the lists hold
    /// nothing that is no longer used.
    #[test]
    fn extra_languages_are_complete() {
        for l in [Lang::De, Lang::Es, Lang::Pl] {
            let list = l.extra().unwrap();
            let mut missing: Vec<&str> =
                TABLE.iter().chain(WORDS).map(|r| r.1).filter(|en| !list.iter().any(|x| x.0 == *en)).collect();
            missing.sort();
            missing.dedup();
            assert!(missing.is_empty(), "{l:?} lacks {} texts: {missing:#?}", missing.len());
            let unused: Vec<&str> =
                list.iter().map(|x| x.0).filter(|en| !TABLE.iter().chain(WORDS).any(|r| r.1 == *en)).collect();
            assert!(unused.is_empty(), "{l:?} has unused texts: {unused:#?}");
        }
    }

    #[test]
    fn no_text_is_listed_twice() {
        let mut seen = std::collections::BTreeSet::new();
        for r in TABLE {
            assert!(seen.insert(r.0), "listed twice: {}", r.0);
        }
    }
}
