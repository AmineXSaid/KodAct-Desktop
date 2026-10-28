//! The interface in other languages (Open Design's `i18n/`, ported as a
//! Rust table, English first). `tr("…")` takes the English and gives it in
//! the language in use: English, French, Spanish or German. The language is
//! the one you picked (the palette's "Language", kept in `language.txt`),
//! else your system's (`LANG`), else English. A string with no translation
//! shows in English; a test checks every `crate::i18n::tr("…")` in the code
//! (always written in full, so it can find them) has all three.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
    Es,
    De,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Fr, Lang::Es, Lang::De];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::De => "de",
        }
    }

    /// Its own name for itself.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        let c = code.trim().to_lowercase();
        Lang::ALL.into_iter().find(|l| c.starts_with(l.code()))
    }
}

static NOW: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    Lang::ALL[NOW.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

pub fn set(l: Lang) {
    NOW.store(
        Lang::ALL.iter().position(|x| *x == l).unwrap_or(0) as u8,
        Ordering::Relaxed,
    );
}

fn choice_file() -> Option<std::path::PathBuf> {
    crate::profile::dir().map(|d| d.join("language.txt"))
}

/// At start: the language you picked, else the system's, else English.
pub fn init() {
    let picked = choice_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|c| Lang::from_code(&c));
    let system = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .and_then(|v| Lang::from_code(&v));
    set(picked.or(system).unwrap_or(Lang::En));
}

/// Use `l` from now on, and next time.
pub fn choose(l: Lang) -> std::io::Result<()> {
    set(l);
    let Some(f) = choice_file() else {
        return Ok(());
    };
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(f, l.code())
}

/// English, then French, Spanish, German.
const TABLE: &[(&str, [&str; 3])] = &[
    // The design gallery.
    ("Design", ["Design", "Diseño", "Design"]),
    (
        "Start from a template, an example, a design system or a skill. Forge sets up the chat, and the page shows beside it as it's made.",
        [
            "Partez d'un modèle, d'un exemple, d'un design system ou d'un savoir-faire. Forge prépare la conversation, et la page s'affiche à côté pendant qu'elle se construit.",
            "Empieza con una plantilla, un ejemplo, un sistema de diseño o una habilidad. Forge prepara el chat y la página aparece al lado mientras se crea.",
            "Starte mit einer Vorlage, einem Beispiel, einem Designsystem oder einem Skill. Forge richtet den Chat ein, und die Seite erscheint daneben, während sie entsteht.",
        ],
    ),
    ("Templates", ["Modèles", "Plantillas", "Vorlagen"]),
    ("Examples", ["Exemples", "Ejemplos", "Beispiele"]),
    ("Design systems", ["Design systems", "Sistemas de diseño", "Designsysteme"]),
    ("Skills", ["Savoir-faire", "Habilidades", "Skills"]),
    ("Preview", ["Aperçu", "Vista previa", "Vorschau"]),
    ("Use", ["Utiliser", "Usar", "Verwenden"]),
    ("Drawing…", ["Dessin…", "Dibujando…", "Wird gezeichnet…"]),
    ("From a website…", ["Depuis un site web…", "Desde un sitio web…", "Von einer Website…"]),
    ("Open a Figma file…", ["Ouvrir un fichier Figma…", "Abrir un archivo de Figma…", "Figma-Datei öffnen…"]),
    ("Show more", ["Afficher plus", "Mostrar más", "Mehr anzeigen"]),
    ("left", ["restants", "restantes", "übrig"]),
    ("in the library", ["dans la bibliothèque", "en la biblioteca", "in der Bibliothek"]),
    ("Reading the design library…", ["Lecture de la bibliothèque de design…", "Leyendo la biblioteca de diseño…", "Designbibliothek wird gelesen…"]),
    ("Nothing matches", ["Aucun résultat", "Nada coincide", "Keine Treffer"]),
    // The share menu and the design bar.
    ("Save as PDF", ["Enregistrer en PDF", "Guardar como PDF", "Als PDF speichern"]),
    ("Save as PNG", ["Enregistrer en PNG", "Guardar como PNG", "Als PNG speichern"]),
    ("Save slides as PNG", ["Enregistrer les diapositives en PNG", "Guardar diapositivas como PNG", "Folien als PNG speichern"]),
    ("Save as PowerPoint", ["Enregistrer en PowerPoint", "Guardar como PowerPoint", "Als PowerPoint speichern"]),
    ("Save as editable PowerPoint", ["Enregistrer en PowerPoint modifiable", "Guardar como PowerPoint editable", "Als bearbeitbares PowerPoint speichern"]),
    ("Save as one HTML file", ["Enregistrer en un seul fichier HTML", "Guardar como un solo archivo HTML", "Als eine HTML-Datei speichern"]),
    ("Save for Figma (editable layers)", ["Enregistrer pour Figma (calques modifiables)", "Guardar para Figma (capas editables)", "Für Figma speichern (bearbeitbare Ebenen)"]),
    ("Open in your browser", ["Ouvrir dans votre navigateur", "Abrir en tu navegador", "Im Browser öffnen"]),
    ("Ask Forge to review the page", ["Demander à Forge de relire la page", "Pedir a Forge que revise la página", "Forge die Seite prüfen lassen"]),
    ("Slides", ["Diapositives", "Diapositivas", "Folien"]),
    ("Saved", ["Enregistré", "Guardado", "Gespeichert"]),
    ("beside the page in", ["à côté de la page en", "junto a la página en", "neben der Seite in"]),
    // Forge's questions.
    ("Forge asks", ["Forge demande", "Forge pregunta", "Forge fragt"]),
    ("Pick any", ["Choix multiples", "Elige varias", "Mehrere möglich"]),
    ("Send answers", ["Envoyer les réponses", "Enviar respuestas", "Antworten senden"]),
    ("Or type your own answer in the box below.", ["Ou écrivez votre propre réponse dans le champ ci-dessous.", "O escribe tu propia respuesta en el cuadro de abajo.", "Oder schreib deine eigene Antwort unten ins Feld."]),
    ("You answered", ["Vous avez répondu", "Respondiste", "Deine Antwort"]),
    ("You didn't answer", ["Vous n'avez pas répondu", "No respondiste", "Keine Antwort"]),
    // The language itself.
    ("Language", ["Langue", "Idioma", "Sprache"]),
];

/// `en` in the language in use.
pub fn tr(en: &'static str) -> &'static str {
    let i = match lang() {
        Lang::En => return en,
        Lang::Fr => 0,
        Lang::Es => 1,
        Lang::De => 2,
    };
    TABLE
        .iter()
        .find(|(k, _)| *k == en)
        .map_or(en, |(_, t)| t[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tr_in_the_code_is_in_the_table() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = vec![];
        let mut files = vec![src.clone()];
        while let Some(p) = files.pop() {
            if p.is_dir() {
                files.extend(std::fs::read_dir(&p).unwrap().flatten().map(|e| e.path()));
                continue;
            }
            if p.extension().is_none_or(|e| e != "rs") || p.ends_with("i18n.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for part in text.split("i18n::tr(\"").skip(1) {
                let Some(end) = part.find("\")") else {
                    continue;
                };
                let key = &part[..end];
                if !TABLE.iter().any(|(k, _)| *k == key) {
                    missing.push(format!("{}: {key}", p.display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "not in the table:\n{}",
            missing.join("\n")
        );
        for (k, t) in TABLE {
            assert!(t.iter().all(|s| !s.is_empty()), "{k}");
        }
    }

    #[test]
    fn languages_switch_and_fall_back() {
        set(Lang::Fr);
        assert_eq!(tr("Templates"), "Modèles");
        assert_eq!(tr("not in the table"), "not in the table");
        set(Lang::De);
        assert_eq!(tr("Use"), "Verwenden");
        set(Lang::En);
        assert_eq!(tr("Templates"), "Templates");
        assert_eq!(Lang::from_code("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("pt_BR"), None);
    }
}
