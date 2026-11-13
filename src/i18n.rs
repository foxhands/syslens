//! Interface languages: Russian (the texts in the code), English and Ukrainian.
//! Texts are written in Russian in the code and translated when shown.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ru,
    En,
    Uk,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ru, Lang::En, Lang::Uk];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
            Lang::Uk => "uk",
        }
    }
    /// Short label for the language button.
    pub fn short(self) -> &'static str {
        match self {
            Lang::Ru => "RU",
            Lang::En => "EN",
            Lang::Uk => "UA",
        }
    }
    /// The language's own name.
    pub fn native(self) -> &'static str {
        match self {
            Lang::Ru => "Русский",
            Lang::En => "English",
            Lang::Uk => "Українська",
        }
    }
    pub fn from_code(c: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == c.trim())
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) {
    CURRENT.store(l as u8, Ordering::Relaxed);
}

pub fn get() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Uk,
        _ => Lang::Ru,
    }
}

/// The Windows display language: Russian, Ukrainian, otherwise English.
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

/// "ru_RU.UTF-8" → Russian, "uk_UA" → Ukrainian, anything else → English.
pub fn from_locale(v: &str) -> Lang {
    match v.get(..2) {
        Some("ru") => Lang::Ru,
        Some("uk") => Lang::Uk,
        _ => Lang::En,
    }
}

/// Russian → English, Ukrainian.
const TABLE: &[(&str, &str, &str)] = &[
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
    ("SysLens-отчёт.txt", "SysLens-report.txt", "SysLens-звіт.txt"),
    ("Отчёт сохранён:", "Report saved:", "Звіт збережено:"),
    ("Не удалось сохранить отчёт:", "Could not save the report:", "Не вдалося зберегти звіт:"),
    ("Не удалось запустить PowerShell:", "Could not start PowerShell:", "Не вдалося запустити PowerShell:"),
    ("Пустой ответ системы", "Empty response from the system", "Порожня відповідь системи"),
    ("Система не ответила вовремя", "The system did not respond in time", "Система не відповіла вчасно"),
    ("Повторить", "Try again", "Повторити"),
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
    ("Поиск на этой странице…", "Search this page…", "Пошук на цій сторінці…"),
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
    ("Предупреждение", "Warning", "Попередження"),
    ("MAC-адрес", "MAC address", "MAC-адреса"),
    ("Скорость", "Speed", "Швидкість"),
    ("Подключён", "Connected", "Підключено"),
    ("Адреса", "Addresses", "Адреси"),
    ("IP-адреса", "IP addresses", "IP-адреси"),
    ("Шлюз", "Gateway", "Шлюз"),
];

/// Units and words inside values.
const WORDS: &[(&str, &str, &str)] = &[
    ("МБ", "MB", "МБ"),
    ("ГБ", "GB", "ГБ"),
    ("КБ", "KB", "КБ"),
    ("ТБ", "TB", "ТБ"),
    ("МГц", "MHz", "МГц"),
    ("ГГц", "GHz", "ГГц"),
    ("из", "of", "з"),
    ("Гц", "Hz", "Гц"),
    ("бит", "bit", "біт"),
    ("Мбит/с", "Mbit/s", "Мбіт/с"),
    ("свободно", "free", "вільно"),
    ("версия", "version", "версія"),
    ("Неизвестно", "Unknown", "Невідомо"),
    ("Майкрософт", "Microsoft", "Microsoft"),
];

fn lookup(table: &'static [(&'static str, &'static str, &'static str)], s: &str, l: Lang) -> Option<&'static str> {
    table.iter().find(|r| r.0 == s).map(|r| match l {
        Lang::En => r.1,
        Lang::Uk => r.2,
        Lang::Ru => r.0,
    })
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
    let mut best: Option<&(&str, &str, &str)> = None;
    for r in TABLE {
        if s.len() > r.0.len() && s.starts_with(r.0) {
            let next = s[r.0.len()..].chars().next().unwrap_or(' ');
            if !next.is_alphanumeric() && best.map_or(true, |b| r.0.len() > b.0.len()) {
                best = Some(r);
            }
        }
    }
    match best {
        Some(r) => format!("{}{}", if l == Lang::En { r.1 } else { r.2 }, &s[r.0.len()..]),
        None => s.to_string(),
    }
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
    // "На диске C: свободно лишь 9%."
    if let Some(rest) = v.strip_prefix("На диске ") {
        if let Some((disk, pct)) = rest.split_once(" свободно лишь ") {
            let pct = pct.trim_end_matches('.');
            return match l {
                Lang::En => format!("Only {pct} free on disk {disk}."),
                _ => format!("На диску {disk} вільно лише {pct}."),
            };
        }
    }
    let mut out: Vec<String> = v.split(' ').map(|w| word(w, l)).collect();
    if key == "Время работы" {
        let (d, h, m, s) = if l == Lang::En { ("d", "h", "min", "s") } else { ("дн.", "год", "хв", "с") };
        for w in out.iter_mut() {
            *w = match w.as_str() {
                "дн." => d.into(),
                "ч" => h.into(),
                "мин" => m.into(),
                "с" => s.into(),
                _ => w.clone(),
            };
        }
    }
    if key == "Напряжение" && l == Lang::En {
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
        assert_eq!(value("x", "CT500P3SSD8  (466 ГБ, NVMe)", Lang::En), "CT500P3SSD8  (466 GB, NVMe)");
        assert_eq!(value("x", "На диске C: свободно лишь 9%.", Lang::En), "Only 9% free on disk C:.");
        assert_eq!(tr("Сборка ОС", Lang::Ru), "Сборка ОС");
        assert_eq!(value("x", "свободно 14.8 ГБ из 150.3 ГБ", Lang::En), "free 14.8 GB of 150.3 GB");
        assert_eq!(value("x", "Ryzen 5  ·  6C/12T  ·  3.7 ГГц", Lang::Uk), "Ryzen 5  ·  6C/12T  ·  3.7 ГГц");
    }
}
