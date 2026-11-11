//! The documents the user adds on the Profile chapter, which Jobdesk copies into the job search folder's `documents/`
//! for `/setup` to read. Only PDFs: ai-job-search's documents guide says `/setup` cannot read a Word file.

use std::path::Path;

use crate::shared_kernel::visible_text::visible_chars;

/// The folder of the job search folder that the copies go into.
pub const DOCUMENTS: &str = "documents";

/// A document larger than this is refused, so a copy never fills the disk or holds the window for long.
pub const MAX_DOCUMENT: u64 = 100 * 1000 * 1000;

/// How many numbered names a copy tries before it gives up on a name that is always taken.
pub const MAX_NUMBER: u32 = 999;

/// Why a file the user added is not taken. Each has its own plain line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A Word or other word processor file.
    Word,
    /// Any other file that is not a PDF.
    NotPdf,
    /// A folder or a special file.
    NotAFile,
    /// A symbolic link, which a copy never follows.
    Link,
    TooLarge,
    /// It could not be opened or read.
    Unreadable,
}

impl Refusal {
    pub fn line(self) -> &'static str {
        match self {
            Self::Word => "Word files can't be read here. Save it as a PDF, then add the PDF.",
            Self::NotPdf => "Only PDF files can be read here.",
            Self::NotAFile => "This is not a file. Add the PDF itself.",
            Self::Link => "This is a link to a file. Add the file itself.",
            Self::TooLarge => "This file is over 100 MB, too large to read.",
            Self::Unreadable => "This file could not be opened.",
        }
    }
}

/// Why a copy into `documents/` did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFailure {
    /// The file changed since it was added, and is now refused.
    Refused(Refusal),
    /// The job search folder, its `documents/` or the kind's subfolder in it is not a real folder, is a link, or cannot
    /// be made.
    NoDocumentsFolder,
    /// Every numbered name is taken.
    NoFreeName,
    /// Reading or writing failed partway. Nothing is left behind.
    Failed,
    /// The disk can give the copy its name in no single step that refuses a taken one: no hard link, no such rename.
    /// Nothing is left behind.
    UnsafeDisk,
}

impl CopyFailure {
    pub fn line(self) -> &'static str {
        match self {
            Self::Refused(refusal) => refusal.line(),
            Self::NoDocumentsFolder => "Your job search folder has no documents folder Jobdesk can copy into.",
            Self::NoFreeName => "Your documents folder already holds too many files of this name.",
            Self::Failed => "The copy did not finish. Nothing was saved.",
            Self::UnsafeDisk => "This disk cannot take the copy safely. Choose a job search folder on your Mac's own disk.",
        }
    }
}

/// What a document is, which decides the subfolder of `documents/` it is copied into. `/setup` reads `documents/` by
/// subfolder, and stops when every one is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Cv,
    LinkedIn,
    Diploma,
    Reference,
    Project,
}

const CV_WORDS: &[&str] = &["cv", "resume", "résumé", "curriculum", "vitae", "lebenslauf"];
const REFERENCE_WORDS: &[&str] = &[
    "reference",
    "references",
    "référence",
    "références",
    "referenz",
    "referenzen",
    "referentie",
    "referenties",
    "referencia",
    "referencias",
    "recommendation",
    "recommandation",
    "recomendación",
    "recomendacion",
    "empfehlung",
    "empfehlungsschreiben",
    "arbeitszeugnis",
    "aanbeveling",
    "testimonial",
];
const DIPLOMA_WORDS: &[&str] = &[
    "diploma",
    "diplomas",
    "diplôme",
    "diplome",
    "diplom",
    "degree",
    "certificate",
    "certificat",
    "certificado",
    "zertifikat",
    "zeugnis",
    "getuigschrift",
    "transcript",
];
const PROJECT_WORDS: &[&str] = &["project", "projects", "projet", "projets", "projekt", "projekte", "proyecto", "proyectos", "portfolio"];

impl DocumentKind {
    /// In the order the menu lists them.
    pub const ALL: [DocumentKind; 5] = [Self::Cv, Self::LinkedIn, Self::Diploma, Self::Reference, Self::Project];

    /// The subfolder of `documents/` that holds this kind: the subfolders ai-job-search v1.7.5's
    /// `.claude/commands/setup.md` Step A1 inventories, `cv/`, `linkedin/`, `diplomas/`, `references/` and `projects/`.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Cv => "cv",
            Self::LinkedIn => "linkedin",
            Self::Diploma => "diplomas",
            Self::Reference => "references",
            Self::Project => "projects",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cv => "CV",
            Self::LinkedIn => "LinkedIn profile",
            Self::Diploma => "Diploma",
            Self::Reference => "Reference letter",
            Self::Project => "Project",
        }
    }

    /// The kind a file's name says, if any. The name without its extension is split into words at every character
    /// that is no letter or digit, and each word is matched whole, in any case, in the user's language or English.
    /// "linkedin" anywhere names a LinkedIn profile, and so does a name whose only word besides numbers is "profile", as
    /// LinkedIn's Save to PDF writes it. When several kinds match, LinkedIn wins, then a reference, a diploma, a CV and
    /// a project, so a CV named after a project role stays a CV.
    pub fn guess(name: &str) -> Option<DocumentKind> {
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_lowercase();
        if stem.contains("linkedin") {
            return Some(Self::LinkedIn);
        }
        let words: Vec<&str> = stem.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).collect();
        let named: Vec<&&str> = words.iter().filter(|word| !word.chars().all(|c| c.is_numeric())).collect();
        if named.len() == 1 && *named[0] == "profile" {
            return Some(Self::LinkedIn);
        }
        let has = |list: &[&str]| words.iter().any(|word| list.contains(word));
        [(REFERENCE_WORDS, Self::Reference), (DIPLOMA_WORDS, Self::Diploma), (CV_WORDS, Self::Cv), (PROJECT_WORDS, Self::Project)]
            .into_iter()
            .find_map(|(list, kind)| has(list).then_some(kind))
    }
}

/// Extensions of word processor files, which get their own line.
const WORD: [&str; 8] = ["doc", "docx", "docm", "dot", "dotx", "odt", "rtf", "pages"];

/// Whether a file of this name may be a document, by its extension, in any case.
pub fn by_name(name: &str) -> Result<(), Refusal> {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "pdf" => Ok(()),
        word if WORD.contains(&word) => Err(Refusal::Word),
        _ => Err(Refusal::NotPdf),
    }
}

/// Whether the first bytes of a file are a PDF's: `%PDF-` within its first KiB, as readers allow.
pub fn looks_like_pdf(start: &[u8]) -> bool {
    start.windows(5).take(1024).any(|window| window == b"%PDF-")
}

/// The name a file shows under, from its path: without hidden characters, so it cannot look like another file's.
pub fn shown_name(path: &Path) -> String {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    visible_chars(&name).collect::<String>().trim().to_owned()
}

/// The longest stem a copy keeps, in bytes, well under what a disk allows for a whole name.
const MAX_STEM: usize = 200;

/// The name a copy of `name` takes in `documents/`: the name as shown, with no character that separates folders or that
/// a disk refuses, no leading dot that would hide it, and the extension `.pdf`. A name left empty becomes `Document.pdf`.
pub fn copy_name(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let stem: String = visible_chars(stem).map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\t') { '-' } else { c }).collect();
    let stem = stem.trim().trim_start_matches('.').trim();
    let mut kept = String::new();
    for c in stem.chars() {
        if kept.len() + c.len_utf8() > MAX_STEM {
            break;
        }
        kept.push(c);
    }
    let kept = kept.trim_end();
    format!("{}.pdf", if kept.is_empty() { "Document" } else { kept })
}

/// `name` with a number added before its extension, for a name already taken: `cv.pdf` and 2 give `cv (2).pdf`.
pub fn numbered(name: &str, number: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem} ({number}).{extension}"),
        None => format!("{name} ({number})"),
    }
}

/// Reads the files the user adds and copies them into the job search folder.
pub trait DocumentCopier: Send + Sync {
    /// Looks at a file the user added, never following a link: a regular PDF it can open, with its page count when the
    /// PDF reads.
    fn look(&self, source: &Path) -> Result<Option<usize>, Refusal>;
    /// Copies `source` into `documents/<kind folder>/` of `folder` under its `copy_name`, numbered when that is taken in
    /// that subfolder, and returns the name it took. It never writes in the `documents/` root or over a file, never
    /// follows a link and never moves or changes `source`.
    fn copy(&self, source: &Path, folder: &Path, kind: DocumentKind) -> Result<String, CopyFailure>;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn only_a_pdf_is_taken_and_a_word_file_has_its_own_line() {
        assert_eq!(by_name("cv.pdf"), Ok(()));
        assert_eq!(by_name("Profile.PDF"), Ok(()));
        assert_eq!(by_name("letter.docx"), Err(Refusal::Word));
        assert_eq!(by_name("letter.DOC"), Err(Refusal::Word));
        assert_eq!(by_name("cv.pages"), Err(Refusal::Word));
        assert_eq!(by_name("scan.png"), Err(Refusal::NotPdf));
        assert_eq!(by_name("pdf"), Err(Refusal::NotPdf));
        assert_eq!(by_name("cv.pdf.zip"), Err(Refusal::NotPdf));
        assert_eq!(Refusal::Word.line(), "Word files can't be read here. Save it as a PDF, then add the PDF.");
    }

    #[test]
    fn a_pdf_starts_with_its_mark() {
        assert!(looks_like_pdf(b"%PDF-1.7\n"));
        assert!(looks_like_pdf(b"\xef\xbb\xbf%PDF-1.4"));
        assert!(!looks_like_pdf(b"PK\x03\x04word/document.xml"));
        assert!(!looks_like_pdf(b""));
    }

    #[test]
    fn a_shown_name_has_no_hidden_character() {
        assert_eq!(shown_name(&PathBuf::from("/Users/sam/Desktop/cv\u{202e}fdp.exe.pdf")), "cvfdp.exe.pdf");
        assert_eq!(shown_name(&PathBuf::from("/Users/sam/Desktop/Profile\u{200b}.pdf")), "Profile.pdf");
    }

    #[test]
    fn a_copy_name_keeps_the_name_and_drops_what_could_leave_the_folder() {
        assert_eq!(copy_name("Sam Rivera CV.pdf"), "Sam Rivera CV.pdf");
        assert_eq!(copy_name("Profile.PDF"), "Profile.pdf");
        assert_eq!(copy_name("cv\u{202e}fdp.pdf"), "cvfdp.pdf");
        assert_eq!(copy_name("..pdf"), "Document.pdf");
        assert_eq!(copy_name(".hidden.pdf"), "hidden.pdf");
        assert_eq!(copy_name("a/b:c.pdf"), "a-b-c.pdf");
        assert_eq!(copy_name(" \u{200b} .pdf"), "Document.pdf");
        let long = copy_name(&format!("{}.pdf", "é".repeat(300)));
        assert!(long.len() <= MAX_STEM + 4 && long.ends_with("é.pdf"), "{long}");
    }

    #[test]
    fn a_kind_is_guessed_from_whole_words_of_the_name() {
        use DocumentKind::*;
        let table = [
            ("Sam Rivera CV.pdf", Some(Cv)),
            ("Resume_2024.pdf", Some(Cv)),
            ("résumé.pdf", Some(Cv)),
            ("CURRICULUM VITAE.pdf", Some(Cv)),
            ("Lebenslauf Sam.pdf", Some(Cv)),
            ("Profile.pdf", Some(LinkedIn)),
            ("Profile (1).pdf", Some(LinkedIn)),
            ("linkedin-export.pdf", Some(LinkedIn)),
            ("SamLinkedInProfile.pdf", Some(LinkedIn)),
            ("My profile summary.pdf", None),
            ("Diplôme.pdf", Some(Diploma)),
            ("Degree certificate.pdf", Some(Diploma)),
            ("Zeugnis.pdf", Some(Diploma)),
            ("Reference letter Tessel.pdf", Some(Reference)),
            ("Arbeitszeugnis Harbor.pdf", Some(Reference)),
            ("Aanbeveling.pdf", Some(Reference)),
            ("Portfolio 2023.pdf", Some(Project)),
            ("Projet final.pdf", Some(Project)),
            ("CV reference Tessel.pdf", Some(Reference)),
            ("Sam Rivera CV - Project Manager.pdf", Some(Cv)),
            ("Resume Senior Project Lead.pdf", Some(Cv)),
            ("cvs.pdf", None),
            ("Scan_0001.pdf", None),
        ];
        for (name, kind) in table {
            assert_eq!(DocumentKind::guess(name), kind, "{name}");
        }
    }

    #[test]
    fn each_kind_has_its_folder_and_its_label_in_menu_order() {
        let folders: Vec<&str> = DocumentKind::ALL.iter().map(|kind| kind.folder()).collect();
        assert_eq!(folders, ["cv", "linkedin", "diplomas", "references", "projects"]);
        let labels: Vec<&str> = DocumentKind::ALL.iter().map(|kind| kind.label()).collect();
        assert_eq!(labels, ["CV", "LinkedIn profile", "Diploma", "Reference letter", "Project"]);
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        assert_eq!(numbered("cv.pdf", 2), "cv (2).pdf");
        assert_eq!(numbered("Sam Rivera CV.pdf", 3), "Sam Rivera CV (3).pdf");
        assert_eq!(numbered("Document", 2), "Document (2)");
    }
}
