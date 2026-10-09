//! Is this process a game? Store/launcher install folders are a strong signal;
//! launchers, crash reporters and anti-cheat helpers that live there are not games.

const GAME_DIRS: [&str; 12] = [
    r"\steamapps\common\",
    r"\epic games\",
    r"\riot games\",
    r"\gog galaxy\games\",
    r"\gog games\",
    r"\xboxgames\",
    r"\ea games\",
    r"\ubisoft game launcher\games\",
    r"\battle.net\",
    r"\call of duty\",
    r"\rockstar games\",
    r"\windowsapps\",
];

const NOT_GAMES: [&str; 22] = [
    "steam", "steamwebhelper", "epicgameslauncher", "epicwebhelper", "riotclientservices", "riotclientux", "riotclientuxrender",
    "battle.net", "agent", "galaxyclient", "galaxyclient helper", "eadesktop", "eabackgroundservice", "upc", "ubisoftconnect",
    "unitycrashhandler64", "unitycrashhandler32", "crashreportclient", "easyanticheat", "beservice", "explorer", "applicationframehost",
];

pub fn is_game(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let stem = std::path::Path::new(&lower).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    if NOT_GAMES.contains(&stem.as_str()) {
        return false;
    }
    // WindowsApps also hosts ordinary Store apps; only count it for known game publishers' packages.
    if lower.contains(r"\windowsapps\") {
        return ["microsoft.minecraft", "bethesda", "xbox", "mojang", "playground", "343industries"].iter().any(|p| lower.contains(p));
    }
    GAME_DIRS.iter().any(|d| lower.contains(d))
}

#[cfg(test)]
mod tests {
    use super::is_game;

    #[test]
    fn detects_store_games_but_not_launchers() {
        assert!(is_game(r"D:\SteamLibrary\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe"));
        assert!(is_game(r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"));
        assert!(!is_game(r"C:\Program Files (x86)\Steam\steam.exe"));
        assert!(!is_game(r"C:\Riot Games\Riot Client\RiotClientServices.exe"));
        assert!(!is_game(r"C:\Program Files\Google\Chrome\Application\chrome.exe"));
        assert!(!is_game(r"C:\Program Files\WindowsApps\Microsoft.WindowsCalculator_11\Calculator.exe"));
    }
}

/// Friendly names for executables profiles commonly list.
pub fn display_name(exe: &str) -> Option<&'static str> {
    const NAMES: [(&str, &str); 22] = [
        ("cs2", "Counter-Strike 2"),
        ("valorant-win64-shipping", "VALORANT"),
        ("r5apex", "Apex Legends"),
        ("r5apex_dx12", "Apex Legends (DX12)"),
        ("fortniteclient-win64-shipping", "Fortnite"),
        ("tslgame", "PUBG"),
        ("overwatch", "Overwatch 2"),
        ("rainbowsix", "Rainbow Six Siege"),
        ("rainbowsix_vulkan", "Rainbow Six Siege (Vulkan)"),
        ("escapefromtarkov", "Escape from Tarkov"),
        ("cod", "Call of Duty"),
        ("modernwarfare", "Call of Duty: MW"),
        ("project8", "Deadlock"),
        ("marvel-win64-shipping", "Marvel Rivals"),
        ("thefinals", "The Finals"),
        ("deltaforceclient-win64-shipping", "Delta Force"),
        ("rustclient", "Rust"),
        ("hunt", "Hunt: Showdown"),
        ("bf2042", "Battlefield 2042"),
        ("bf6", "Battlefield 6"),
        ("ms-teams", "Microsoft Teams"),
        ("zoom", "Zoom"),
    ];
    NAMES.iter().find(|(e, _)| e.eq_ignore_ascii_case(exe)).map(|(_, n)| *n)
}
