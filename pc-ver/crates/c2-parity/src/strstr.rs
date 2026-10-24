//! A deliberately faithful copy of the original `_RES.TXT` reader
//! (`Resources.cpp: LoadResourcesScript`, `ReadWeapons`, `ReadCharacters`),
//! substring key matching and all, used to find any shipped file where our
//! exact-key parser in `c2_formats::res_txt` would behave differently.

use c2_formats::res_txt::{c_atof, c_atoi, CharacterDef, ResData, WeaponDef};

/// Lines as `fgets` returns them in Windows text mode: CRLF → LF, newline kept.
fn fgets_lines(text: &str) -> Vec<String> {
    let t = text.replace("\r\n", "\n");
    t.split_inclusive('\n').map(str::to_string).collect()
}

/// `value = strstr(line, "'"); value[strlen(value)-2] = 0; strcpy(dst, &value[1]);`
fn c_string(line: &str) -> Option<String> {
    let v = &line[line.find('\'')?..];
    let b = v.as_bytes();
    let end = b.len().checked_sub(2)?;
    Some(String::from_utf8_lossy(b.get(1..end.max(1))?).into_owned())
}

pub struct Faithful {
    pub data: ResData,
    /// Places where the original would `DoHalt("Script loading error")`.
    pub halts: Vec<String>,
}

pub fn parse(text: &str) -> Faithful {
    let lines = fgets_lines(text);
    let mut it = lines.iter();
    let mut out = Faithful {
        data: ResData::default(),
        halts: Vec::new(),
    };
    while let Some(line) = it.next() {
        if line.starts_with('.') {
            break;
        }
        if line.contains("weapons") {
            read_blocks(&mut it, &mut out.halts, &mut out.data.weapons, weapon_line);
        }
        if line.contains("characters") {
            read_blocks(
                &mut it,
                &mut out.halts,
                &mut out.data.characters,
                character_line,
            );
        }
    }
    out
}

/// `while fgets { if '}' break; if '{' { while fgets { if '}' {count++; break}; ... } } }`
fn read_blocks<'a, T: Default>(
    it: &mut impl Iterator<Item = &'a String>,
    halts: &mut Vec<String>,
    list: &mut Vec<T>,
    on_line: fn(&mut T, &str, &str),
) {
    while let Some(line) = it.next() {
        if line.contains('}') {
            break;
        }
        if line.contains('{') {
            list.push(T::default());
            for line in it.by_ref() {
                if line.contains('}') {
                    break;
                }
                match (line.find('='), list.last_mut()) {
                    (Some(i), Some(t)) => on_line(t, line, &line[i + 1..]),
                    _ => halts.push(format!("line without '=': {:?}", line.trim_end())),
                }
            }
        }
    }
}

fn weapon_line(w: &mut WeaponDef, line: &str, value: &str) {
    let has = |k| line.contains(k);
    if has("power") {
        w.power = Some(c_atof(value))
    }
    if has("prec") {
        w.prec = Some(c_atof(value))
    }
    if has("loud") {
        w.loud = Some(c_atof(value))
    }
    if has("rate") {
        w.rate = Some(c_atof(value))
    }
    if has("shots") {
        w.shots = Some(c_atoi(value))
    }
    if has("reload") {
        w.reload = Some(c_atoi(value))
    }
    if has("trace") {
        w.trace = Some(c_atoi(value))
    }
    if has("optic") {
        w.optic = Some(c_atoi(value) as f32)
    }
    if has("fall") {
        w.fall = Some(c_atoi(value))
    }
    if has("name") {
        w.name = c_string(line)
    }
    if has("file") {
        w.file = c_string(line)
    }
    if has("pic") {
        w.pic = c_string(line)
    }
}

fn character_line(c: &mut CharacterDef, line: &str, value: &str) {
    let has = |k| line.contains(k);
    if has("mass") {
        c.mass = Some(c_atof(value))
    }
    if has("length") {
        c.length = Some(c_atof(value))
    }
    if has("radius") {
        c.radius = Some(c_atof(value))
    }
    if has("health") {
        c.health = Some(c_atoi(value))
    }
    if has("basescore") {
        c.basescore = Some(c_atoi(value))
    }
    if has("ai") {
        c.ai = Some(c_atoi(value))
    }
    if has("smell") {
        c.smell = Some(c_atof(value))
    }
    if has("hear") {
        c.hear = Some(c_atof(value))
    }
    if has("look") {
        c.look = Some(c_atof(value))
    }
    if has("shipdelta") {
        c.shipdelta = Some(c_atof(value))
    }
    if has("scale0") {
        c.scale0 = Some(c_atoi(value))
    }
    if has("scaleA") {
        c.scale_a = Some(c_atoi(value))
    }
    if has("danger") {
        c.danger = true
    }
    if has("name") {
        c.name = c_string(line)
    }
    if has("file") {
        c.file = c_string(line)
    }
    if has("pic") {
        c.pic = c_string(line)
    }
}

/// Field-by-field differences between the faithful reading and ours.
pub fn diff(ours: &ResData, theirs: &Faithful) -> Vec<String> {
    let mut d: Vec<String> = theirs
        .halts
        .iter()
        .map(|h| format!("original would halt: {h}"))
        .collect();
    if ours.weapons.len() != theirs.data.weapons.len() {
        d.push(format!(
            "weapon count {} vs original {}",
            ours.weapons.len(),
            theirs.data.weapons.len()
        ));
    }
    if ours.characters.len() != theirs.data.characters.len() {
        d.push(format!(
            "character count {} vs original {}",
            ours.characters.len(),
            theirs.data.characters.len()
        ));
    }
    macro_rules! cmp {
        ($kind:literal, $i:expr, $a:expr, $b:expr, $($f:ident),*) => {$(
            if $a.$f != $b.$f {
                d.push(format!("{} #{} {:?}: {} = {:?}, original {:?}", $kind, $i, $a.name, stringify!($f), $a.$f, $b.$f));
            }
        )*};
    }
    for (i, (a, b)) in ours.weapons.iter().zip(&theirs.data.weapons).enumerate() {
        cmp!(
            "weapon", i, a, b, name, file, pic, power, prec, loud, rate, shots, reload, trace,
            optic, fall
        );
    }
    for (i, (a, b)) in ours
        .characters
        .iter()
        .zip(&theirs.data.characters)
        .enumerate()
    {
        cmp!(
            "character",
            i,
            a,
            b,
            name,
            file,
            pic,
            ai,
            health,
            mass,
            length,
            radius,
            basescore,
            smell,
            hear,
            look,
            shipdelta,
            scale0,
            scale_a,
            danger
        );
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_formats::res_txt::ResTxt;

    #[test]
    fn detects_substring_key_bug_and_halts() {
        let text = "characters {\r\n{\r\n ai = 12\r\n name = 'Maiasaura'\r\n}\r\n{\r\n name = 'X'\r\n\r\n}\r\n}\r\n.\r\n";
        let ours = ResTxt::parse(text).unwrap().to_data();
        let theirs = parse(text);
        assert_eq!(
            theirs.data.characters[0].ai,
            Some(0),
            "original resets ai via 'Maiasaura'"
        );
        assert_eq!(theirs.data.characters[0].name.as_deref(), Some("Maiasaura"));
        let d = diff(&ours, &theirs);
        assert!(
            d.iter()
                .any(|l| l.contains("ai = Some(12), original Some(0)")),
            "{d:?}"
        );
        assert!(d.iter().any(|l| l.contains("original would halt")), "{d:?}");
    }

    #[test]
    fn agrees_on_well_formed_input() {
        let text = "weapons {\r\n{\r\n name  = 'Pistol'\r\n file  = 'pistol.car'\r\n power = 2\r\n trace = 6\r\n}\r\n}\r\n.\r\n";
        let ours = ResTxt::parse(text).unwrap().to_data();
        assert_eq!(diff(&ours, &parse(text)), Vec::<String>::new());
    }
}
