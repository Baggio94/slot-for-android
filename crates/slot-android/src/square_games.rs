//! The Master System and Mega Drive games whose play is drawn 256 pixels
//! wide (Mega Drive H32, a Master System game's 224 lines): shown in square
//! pixels, 8:7, on screens narrower than 4:3 (Siham 2026-11-23).
//! Generated from FirebrandX's "Definitive 256 mode Genesis Games List"
//! (USA, 2019), its European and Japanese releases and the ones it misses
//! (Sega-16 forum thread), and SMS Power's three Codemasters games in 224
//! lines, matched against the No-Intro DATs of libretro-database.

/// Mega Drive header serials (0x180: the type and the revision left out),
/// for every dump, hack and translation that keeps it.
pub const MD_SERIALS: &[(&str, &str)] = &[
    ("00001022", "Flicky"),
    ("00001123", "Turbo OutRun"),
    ("00001135", "Ayrton Senna's Super Monaco GP II"),
    ("00004039", "Arrow Flash"),
    ("00005514", "Ayrton Senna's Super Monaco GP II"),
    ("4026", "Super Monaco GP"),
    ("ACLD008", "Test Drive II - The Duel"),
    ("ACLD014", "WarpSpeed"),
    ("G-4023", "Zoom!"),
    ("G-4032", "DJ Boy"),
    ("G-4046", "Bonanza Bros."),
    ("G-4060", "Wonder Boy V - Monster World III"),
    ("G-4100", "Garou Densetsu - Shukumei no Tatakai"),
    ("G-5506", "Shining and the Darkness"),
    ("G-5512", "Shining Force - Kamigami no Isan"),
    ("G-5519", "Monster World IV"),
    ("G-5521", "Shining Force II - Inishie no Fuuin"),
    ("G-7001", "Virtua Racing"),
    ("G-9999", "Monster World IV"),
    ("MK-1031", "Captain America and the Avengers"),
    ("MK-1083", "Fatal Fury"),
    ("MK-1090", "Street Fighter II' - Special Champion Edition"),
    ("MK-1118", "OutRun 2019"),
    ("MK-1120", "G-LOC - Air Battle"),
    ("MK-1124", "Bonanza Bros."),
    ("MK-1128", "Twin Cobra - Desert Attack Helicopter"),
    ("MK-1131", "Phelios"),
    ("MK-1139", "DJ Boy"),
    ("MK-1201", "Pat Riley Basketball"),
    ("MK-12056", "Super Street Fighter II"),
    ("MK-1229", "Virtua Racing"),
    ("MK-1310", "Shining in the Darkness"),
    ("MK-1312", "Shining Force"),
    ("MK-1313", "Star Trek - The Next Generation - Echoes from the Past"),
    ("MK-1315", "Shining Force II"),
    ("MK-1541", "Snake Rattle n Roll"),
    ("MK-1570", "Mighty Morphin Power Rangers"),
    ("MK-1573", "Mega Bomberman"),
    ("MK-1704", "Lemmings"),
    ("MK-1840", "Bugs Bunny in Double Trouble"),
    ("T-015096", "Bubble and Squeak"),
    ("T-081036", "Krusty's Super Fun House"),
    ("T-081086", "WWF Super WrestleMania"),
    ("T-081116", "George Foreman's KO Boxing"),
    ("T-081126", "NBA All-Star Challenge"),
    ("T-081136", "Spider-Man - X-Men - Arcade's Revenge"),
    ("T-081166", "Double Dragon 3 - The Arcade Game"),
    ("T-081176", "T2 - Terminator 2 - Judgment Day"),
    ("T-081196", "WWF Royal Rumble"),
    ("T-081216", "Simpsons, The - Bart's Nightmare"),
    ("T-081236", "Addams Family, The"),
    ("T-081256", "Spider-Man - Venom - Maximum Carnage"),
    ("T-081316", "WWF RAW"),
    ("T-081366", "Newman Haas IndyCar Featuring Nigel Mansell"),
    ("T-088066", "Formula One"),
    ("T-093046", "ESPN Sunday Night NFL"),
    ("T-100026", "Wayne's World"),
    ("T-100076", "seaQuest DSV"),
    ("T-101046", "Pink Goes to Hollywood"),
    ("T-103016", "Fatal Fury"),
    ("T-103046", "Fatal Fury 2"),
    ("T-106153", "LHX Attack Chopper"),
    ("T-106193", "Space Funky B.O.B."),
    ("T-107016", "Best of the Best - Championship Karate"),
    ("T-11046", "Ultimate QIX"),
    ("T-11076", "Growl"),
    ("T-11103", "Volfied"),
    ("T-11106", "Chase H.Q. II"),
    ("T-11123", "Fire Mustang"),
    ("T-11153", "Runark"),
    ("T-112016", "Tom and Jerry - Frantic Antics!"),
    ("T-112026", "Barbie Super Model"),
    ("T-113076", "3 Ninjas Kick Back"),
    ("T-113086", "No Escape"),
    ("T-119026", "Bubsy in - Claws Encounters of the Furred Kind"),
    ("T-119046", "Pele!"),
    ("T-119066", "Barkley Shut Up and Jam!"),
    ("T-119186", "Barkley Shut Up and Jam 2"),
    ("T-12016", "Street Fighter II' - Special Champion Edition"),
    ("T-12043", "Super Street Fighter II - The New Challengers"),
    ("T-12056", "Super Street Fighter II"),
    ("T-12066", "Saturday Night Slammasters"),
    ("T-12076", "Great Circus Mystery Starring Mickey & Minnie, The"),
    ("T-125036", "ClayFighter"),
    ("T-130026", "Radical Rex"),
    ("T-13016", "Midnight Resistance"),
    ("T-13043", "Midnight Resistance"),
    ("T-13056", "Captain America and the Avengers"),
    ("T-137036", "Chavez II"),
    ("T-139016", "Beavis and Butt-Head"),
    ("T-14013", "Phelios"),
    ("T-14036", "Powerball"),
    ("T-14086", "Splatterhouse 3"),
    ("T-14116", "Pac-Attack"),
    ("T-14126", "Pac-Man 2 - The New Adventures"),
    ("T-14136", "WeaponLord"),
    ("T-146023", "Garou Densetsu 2 - Aratanaru Tatakai"),
    ("T-148086", "Primal Rage"),
    ("T-148096", "Primal Rage"),
    ("T-15063", "Lemmings"),
    ("T-158016", "NCAA Final Four Basketball"),
    ("T-161026", "Tick, The"),
    ("T-164046", "Addams Family Values"),
    ("T-164056", "Adventures of Mighty Max, The"),
    ("T-172036", "NHL 96"),
    ("T-172146", "NHL 97"),
    ("T-172176", "NHL 98"),
    ("T-20043", "Verytex"),
    ("T-23026", "King Salmon - The Big Catch"),
    ("T-23066", "Top Gear 2"),
    ("T-24033", "Kyuukyoku Tiger"),
    ("T-24036", "Task Force Harrier EX"),
    ("T-24053", "Task Force Harrier EX"),
    ("T-24056", "Fighting Masters"),
    ("T-24063", "Fighting Masters"),
    ("T-24096", "Viewpoint"),
    ("T-28023", "Insector X"),
    ("T-28026", "Insector X"),
    ("T-29016", "CrossFire"),
    ("T-29023", "CrossFire"),
    ("T-30013", "Super Volley Ball"),
    ("T-32013", "Final Zone"),
    ("T-32043", "Dino Land"),
    ("T-33013", "Aero Blasters"),
    ("T-33016", "DJ Boy"),
    ("T-33026", "Air Buster"),
    ("T-33033", "Deadly Moves"),
    ("T-33056", "Chester Cheetah - Too Cool to Fool"),
    ("T-33066", "Chester Cheetah - Wild Wild Quest"),
    ("T-36016", "Tecmo Super Bowl"),
    ("T-36023", "Tecmo Super Bowl"),
    ("T-36026", "Tecmo Super NBA Basketball"),
    ("T-36033", "Tecmo Super NBA Basketball"),
    ("T-36036", "Tecmo Super Hockey"),
    ("T-36046", "Tecmo Super Baseball"),
    ("T-44033", "OutRun 2019"),
    ("T-47066", "Devilish - The Next Possession"),
    ("T-48036", "Ms. Pac-Man"),
    ("T-48336", "Lawnmower Man, The"),
    ("T-48356", "Kawasaki Superbike Challenge"),
    ("T-48366", "Mega SWIV"),
    ("T-48426", "Primal Rage"),
    ("T-49023", "Valis III"),
    ("T-49083", "Mugen Senshi Valis"),
    ("T-49086", "Valis III"),
    ("T-49086-", "Valis III"),
    ("T-49103", "SD Valis"),
    ("T-49116", "Beast Wrestler"),
    ("T-49126", "Syd of Valis"),
    ("T-49126-", "Syd of Valis"),
    ("T-49186", "Exile"),
    ("T-49196", "Valis"),
    ("T-49196-", "Valis"),
    ("T-49206", "Sol-Deace"),
    ("T-50236", "EA Hockey"),
    ("T-50396", "NHLPA Hockey 93"),
    ("T-50416", "LHX Attack Chopper"),
    ("T-50456", "Shadow of the Beast II"),
    ("T-50476", "Lotus Turbo Challenge"),
    ("T-50586", "B.O.B."),
    ("T-50656", "NHL '94"),
    ("T-50716", "F-117 Night Storm"),
    ("T-50746", "Lotus II"),
    ("T-50856", "NHL 95"),
    ("T-50896", "Sesame Street Counting Cafe"),
    ("T-70066", "Global Gladiators"),
    ("T-70096", "Muhammad Ali Heavyweight Boxing"),
    ("T-70106", "Another World"),
    ("T-70146", "Caesars Palace"),
    ("T-70186", "Chi Chi's Pro Challenge Golf"),
    ("T-70296", "Jammit"),
    ("T-76016", "Romance of the Three Kingdoms II"),
    ("T-76023", "Sangokushi II"),
    ("T-76026", "Uncharted Waters"),
    ("T-76036", "Gemfire"),
    ("T-76046", "Aerobiz"),
    ("T-76053", "Daikoukai Jidai"),
    ("T-76056", "P.T.O. - Pacific Theater of Operations"),
    ("T-76063", "Royal Blood"),
    ("T-76066", "Genghis Khan II - Clan of the Gray Wolf"),
    ("T-76073", "Teitoku no Ketsudan"),
    ("T-76076", "Nobunaga's Ambition"),
    ("T-76083", "Air Management - Oozora ni Kakeru"),
    ("T-76086", "Romance of the Three Kingdoms III - Dragon of Destiny"),
    ("T-76093", "Sangokushi III"),
    ("T-76096", "Operation Europe - Path to Victory 1939-45"),
    ("T-76103", "Aoki Ookami to Shiroki Mejika - Genchou Hishi"),
    ("T-76106", "Liberty or Death"),
    ("T-76113", "Europa Sensen"),
    ("T-76126", "New Horizons"),
    ("T-76136", "Aerobiz Supersonic"),
    ("T-76143", "Air Management II - Koukuuou o Mezase"),
    ("T-76163", "Daikoukai Jidai II"),
    ("T-79066", "Flashback - The Quest for Identity"),
    ("T-81106", "Roger Clemens' MVP Baseball"),
    ("T-81206", "Virtual Bart"),
    ("T-81336", "Batman Forever"),
    ("T-81386", "True Lies"),
    ("T-81506", "Foreman for Real"),
    ("T-81516", "CutThroat Island"),
    ("T-81526", "Scooby-Doo Mystery"),
    ("T-81546", "WWF WrestleMania - The Arcade Game"),
    ("T-81556", "Venom - Spider-Man - Separation Anxiety"),
    ("T-83016", "Wheel of Fortune"),
    ("T-83066", "Nigel Mansell's World Championship Racing"),
    ("T-86016", "Garry Kitchen's Super Battletank - War in the Gulf"),
    ("T-86036", "David Crane's Amazing Tennis"),
    ("T-86046", "Toys - Let the Toy Wars begin!"),
    ("T-86056", "Adventures of Rocky and Bullwinkle and Friends, The"),
    ("T-86066", "Goofy's Hysterical History Tour"),
    ("T-87036", "Outlander"),
    ("T-87056", "Miracle Piano Teaching System, The"),
    ("T-87066", "Super Battleship - The Classic Naval Combat Game"),
    ("T-87076", "Championship Pool"),
    ("T-87106", "NCAA Football"),
    ("T-880660", "F1"),
    ("T-89016", "Clue"),
    ("T-89026", "Monopoly"),
    ("T-93026", "Hook"),
    ("T-93036", "Cliffhanger"),
    ("T-93056", "ESPN Baseball Tonight"),
    ("T-93066", "Last Action Hero"),
    ("T-93086", "Mary Shelley's Frankenstein"),
    ("T-93126", "Fun 'n Games"),
    ("T-93166", "ESPN Speed World"),
    ("T-93176", "ESPN National Hockey Night"),
    ("T-95013", "Teenage Mutant Ninja Turtles - Return of the Shredder"),
    ("T-95016", "Teenage Mutant Hero Turtles - The Hyperstone Heist"),
    ("T-95026", "Sunset Riders"),
    ("T-95073", "Lethal Enforcers"),
    ("T-95096", "Lethal Enforcers"),
    ("T-95126", "Double Dribble - The Playoff Edition"),
    ("T-95136", "Lethal Enforcers II - Gun Fighters"),
    ("T-95186", "Nigel Mansell's World Championship Racing"),
    ("T-97036", "Pro Quarterback"),
    ("T-97046", "Championship Pro-Am"),
    ("T-97056", "Fun 'n' Games"),
    ("T-97066", "Battletoads-Double Dragon"),
    ("T-97106", "Double Dragon V - The Shadow Falls"),
    ("T-XXXXXX-XX", "Addams Family Values"),
];

/// CRC32 of every No-Intro dump of these Mega Drive games (some have no
/// serial, or one other games share).
pub const MD_CRCS: &[u32] = &[
    0x00F05D07, 0x0288F3E1, 0x081012F0, 0x0894D8FB, 0x08FA5A3F, 0x09CBD94C,
    0x0A4F48C3, 0x0BDCCC83, 0x0C661369, 0x0C70B0FF, 0x0CA5BB64, 0x0D901482,
    0x0E0107F1, 0x0EAC7440, 0x0F347015, 0x0FF33054, 0x10BE1D93, 0x11C79320,
    0x1233A229, 0x13ABC2B2, 0x13BC5B72, 0x13FE08A1, 0x143697ED, 0x14744883,
    0x14DC2668, 0x15357DDE, 0x157332B6, 0x165DEFBF, 0x187C6AF6, 0x18C09468,
    0x1996E394, 0x1AAF7707, 0x1AEF72EA, 0x1B003498, 0x1B1754CB, 0x1B569DC2,
    0x1BF67A07, 0x1D8EE010, 0x202ABAA8, 0x20D1AD4C, 0x224FF103, 0x227A1178,
    0x24431625, 0x24D7507C, 0x24F1A3BB, 0x250E3EC3, 0x253512CF, 0x25B05480,
    0x2641653F, 0x272153FB, 0x27E59E35, 0x28655F23, 0x2884C6D1, 0x28C4A006,
    0x29D0D066, 0x2ADB0364, 0x2AF4427F, 0x2C48E712, 0x2C708248, 0x2DA36E01,
    0x2DEDE3DB, 0x2E6EEC7E, 0x2F75E896, 0x2FB4EABA, 0x3044460C, 0x30B512EE,
    0x321BB6BD, 0x337CDA6D, 0x35CBD237, 0x365305A2, 0x36A3AAA4, 0x37DC0108,
    0x39BE80EC, 0x3D36135B, 0x3D842478, 0x3D9318A7, 0x3E30D365, 0x3EE2BBC4,
    0x3FE3D63B, 0x401DC618, 0x40F17BB3, 0x425132F0, 0x4291C8AB, 0x42B8B1C2,
    0x43225612, 0x437BA326, 0x4391175E, 0x44EE5F20, 0x45416AA9, 0x4582817B,
    0x45A50F96, 0x47C6E2D3, 0x4815E075, 0x48993DC3, 0x48FB7E7C, 0x496AF51C,
    0x4A4414EA, 0x4BD6667D, 0x4BFE045C, 0x4D2785BC, 0x4D49A166, 0x4D89E66B,
    0x4E1CC833, 0x4EDAEC59, 0x50081A0B, 0x512ADE32, 0x51D9A84A, 0x53913991,
    0x53A293B5, 0x543BED30, 0x54AB3BEB, 0x54E4CFF1, 0x55F13A00, 0x56976261,
    0x56D41136, 0x5840FFAA, 0x59A2A368, 0x59C71866, 0x5A652458, 0x5A8ABE51,
    0x5BC0DBB8, 0x5C218C6A, 0x5CB3536A, 0x5E19C1C0, 0x5F51983B, 0x5F86DDC9,
    0x5FC96D96, 0x5FE351B8, 0x60AF0F76, 0x61276D21, 0x61E9C309, 0x631CC8E9,
    0x6384A63C, 0x63C461E2, 0x63FBF497, 0x64446B77, 0x679C41DE, 0x682C192F,
    0x68C70362, 0x6A1A4579, 0x6BC57B2C, 0x6E5AEE10, 0x70626304, 0x70C3428D,
    0x715158A9, 0x71AA889E, 0x71F58614, 0x724D6965, 0x725018EE, 0x731FA4A1,
    0x7600E829, 0x7672EFA5, 0x7753A296, 0x78C2F046, 0x78D4640A, 0x79587507,
    0x79F33EB6, 0x7ADF232F, 0x7B344D0C, 0x7B64CD98, 0x7BB9DD9B, 0x7C594E1A,
    0x7E1A324A, 0x7E41C8FE, 0x7F96E663, 0x7FFBD1AD, 0x8135702C, 0x81C9662B,
    0x8239DD17, 0x82991B5F, 0x82FE2D3F, 0x8352B1D0, 0x83699E34, 0x83CB46D1,
    0x84935BC3, 0x860E749A, 0x86151BF1, 0x87A281AE, 0x87DA801A, 0x8A60468E,
    0x8C5C93B8, 0x8C6E2AA3, 0x8CA23838, 0x8DB9F378, 0x8F3F6E4D, 0x8FA0B6E6,
    0x8FDAA9BB, 0x90C6E20C, 0x90F9BAB3, 0x9177088C, 0x92ED7344, 0x9377F1B5,
    0x9378FBCD, 0x93E509EA, 0x9438F5DD, 0x94596174, 0x9624D4EF, 0x9625C434,
    0x966D5286, 0x96D8440C, 0x9789EA79, 0x98D49170, 0x99CA1BFB, 0x9B08E4E4,
    0x9B5C28EA, 0x9BFC279C, 0x9C3973A4, 0x9CBF44D3, 0x9DCDC894, 0x9EDDEB3D,
    0x9FEFB1AB, 0xA00DA987, 0xA154433A, 0xA3CF6E9C, 0xA5D023F9, 0xA719542E,
    0xA77E4E9F, 0xA7994615, 0xA7CACD59, 0xA81527B7, 0xA8DE6AEA, 0xA920C852,
    0xAC30C297, 0xAD1DBED8, 0xADF6476C, 0xAF041BE6, 0xAF050C46, 0xB0416C60,
    0xB0B5E3C9, 0xB0C5E3F7, 0xB12C1BC1, 0xB1823595, 0xB360704B, 0xB36AB75C,
    0xB3A67ED5, 0xB3DB0C71, 0xB496DE28, 0xB5804771, 0xB5AE351D, 0xB638B6A3,
    0xB69DC53E, 0xB7278199, 0xB906B992, 0xB929D6C5, 0xB97B735D, 0xB9895365,
    0xB9BC07BC, 0xBA6EAECA, 0xBAC87612, 0xBAFC375F, 0xBD3074D2, 0xBD556381,
    0xBD5933EE, 0xBE91B28A, 0xBFDE41C8, 0xC10268DA, 0xC3D6A5D4, 0xC4674ADF,
    0xC6AAC589, 0xC6BC06FD, 0xC7B6435E, 0xC7F6D0B6, 0xC8BB0257, 0xC8D8EFC3,
    0xC9E3F2DC, 0xCA2BF99D, 0xCBBF4262, 0xCBC9951B, 0xCC73F3A9, 0xCC8B2B69,
    0xCCD73738, 0xCEC0A0BB, 0xCFAA9BCE, 0xD02D3282, 0xD26A32B6, 0xD2BBB00E,
    0xD2CF6EBE, 0xD3F300AC, 0xD49F8444, 0xD677EDD1, 0xD83369D9, 0xD8EEB2BD,
    0xD91B52B8, 0xD9A364FF, 0xD9D4C6E2, 0xDA4AB3CD, 0xDB41F2D5, 0xDC9F02DB,
    0xE040F0DA, 0xE0639CA2, 0xE1FDC787, 0xE32E17E2, 0xE5A24999, 0xE5CF560D,
    0xE5FDD28B, 0xE7014092, 0xE7CBA1D8, 0xE82ACDFC, 0xE8EE917E, 0xE9310D3B,
    0xE9742041, 0xE9A54EED, 0xEA6E8AFC, 0xEAA2ACB7, 0xEAC8DED6, 0xEAD69824,
    0xEB5E0B28, 0xEB7E36C3, 0xEBABBC70, 0xEBD6D5BA, 0xED3F3E20, 0xEF02D57B,
    0xEF840EF2, 0xEFA0D2CB, 0xF015C2AD, 0xF067C103, 0xF25F1E49, 0xF2AF886E,
    0xF361D0BF, 0xF39E4BF2, 0xF3D65BAA, 0xF4169404, 0xF4726918, 0xF50BE478,
    0xF516E7D9, 0xF5D7B948, 0xF60EF143, 0xF764005E, 0xF842240B, 0xF9BDF8C5,
    0xFBB1E120, 0xFBDD4520, 0xFC962CFC, 0xFE6F2350, 0xFE7A7ED1, 0xFF0DCD3C,
    0xFF9E9F45,
];

/// Cosmic Spacehead, Fantastic Dizzy and Micro Machines: Codemasters' Master
/// System games in 224 lines.
pub const SMS_CRCS: &[u32] = &[
    0x29822980, 0xA577CE46, 0xB9664AE1,
];

use std::path::Path;

use slot_store::Platform;

/// Whether `rom` is one of these games: its Mega Drive header serial, else
/// its CRC32 (the whole file, read once at launch: under a millisecond).
pub fn listed(platform: Platform, rom: &Path) -> bool {
    let crcs = match platform {
        Platform::Md => MD_CRCS,
        Platform::Sms => SMS_CRCS,
        _ => return false,
    };
    let Ok(bytes) = std::fs::read(rom) else { return false };
    if platform == Platform::Md && header_serial(&bytes)
        .is_some_and(|serial| MD_SERIALS.iter().any(|(listed, _)| *listed == serial)) {
        return true;
    }
    let mut crc = flate2::Crc::new();
    crc.update(&bytes);
    crcs.binary_search(&crc.sum()).is_ok()
}

/// The serial at 0x180 ("GM T-081236 -00"): its type and revision left out,
/// spaces removed ("T-081236"), as the list keeps it.
fn header_serial(rom: &[u8]) -> Option<String> {
    let field = rom.get(0x180..0x18E)?;
    let text: String = field.get(2..)?.iter().map(|&b| b as char).filter(|c| *c != ' ').collect();
    if !text.is_ascii() { return None; }
    let text = match text.rsplit_once('-') {
        Some((head, rev)) if rev.len() == 2 && rev.bytes().all(|b| b.is_ascii_digit()) => head.to_owned(),
        _ => text,
    };
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_are_sorted_for_lookups() {
        assert!(MD_CRCS.windows(2).all(|w| w[0] < w[1]));
        assert!(SMS_CRCS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(SMS_CRCS.len(), 3);
    }

    #[test]
    fn a_header_serial_is_read_as_the_list_keeps_it() {
        let mut rom = vec![0u8; 0x200];
        rom[0x180..0x18E].copy_from_slice(b"GM MK-1312 -00");
        assert_eq!(header_serial(&rom).as_deref(), Some("MK-1312"));
        assert!(MD_SERIALS.iter().any(|(s, title)| *s == "MK-1312" && *title == "Shining Force"));
        rom[0x180..0x18E].copy_from_slice(b"GM 00001009-00");
        assert_eq!(header_serial(&rom).as_deref(), Some("00001009"));
        assert!(!MD_SERIALS.iter().any(|(s, _)| *s == "00001009"), "Sonic is 320 wide");
    }

    #[test]
    fn a_listed_game_is_found_by_serial_or_by_crc() {
        let dir = std::env::temp_dir().join(format!("slot-square-games-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("game.md");
        let mut bytes = vec![0u8; 0x400];
        bytes[0x180..0x18E].copy_from_slice(b"GM MK-1312 -00");
        std::fs::write(&rom, &bytes).unwrap();
        assert!(listed(Platform::Md, &rom), "by serial");
        bytes[0x180..0x18E].copy_from_slice(b"GM 00001009-00");
        std::fs::write(&rom, &bytes).unwrap();
        assert!(!listed(Platform::Md, &rom));
        assert!(!listed(Platform::Snes, &rom));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
