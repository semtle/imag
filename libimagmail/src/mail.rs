use std::result::Result as RResult;
use std::path::Path;
use std::path::PathBuf;
use std::fs::File;
use std::io::Read;
use std::error::Error;
use std::marker::PhantomData;

use libimagstore::store::{FileLockEntry, Store};
use libimagref::reference::Ref;
use libimagref::flags::RefFlags;

use mailparse::{MailParseError, ParsedMail, parse_mail};

use hasher::MailHasher;
use result::Result;
use error::{MapErrInto, MailErrorKind as MEK};

struct Buffer(String);

impl Buffer {
    pub fn parsed<'a>(&'a self) -> RResult<ParsedMail<'a>, MailParseError> {
        parse_mail(self.0.as_bytes())
    }
}

impl From<String> for Buffer {
    fn from(data: String) -> Buffer {
        Buffer(data)
    }
}

pub struct Mail<'a>(Ref<'a>, Buffer);

impl<'a> Mail<'a> {

    /// Imports a mail from the Path passed
    pub fn import_from_path<P: AsRef<Path>>(store: &Store, p: P) -> Result<Mail> {
        let h = MailHasher::new();
        let f = RefFlags::default().with_content_hashing(true).with_permission_tracking(false);
        let p = PathBuf::from(p.as_ref());

        Ref::create_with_hasher(store, p, f, h)
            .map_err_into(MEK::RefCreationError)
            .and_then(|reference| {
                reference.fs_file()
                    .map_err_into(MEK::RefHandlingError)
                    .and_then(|path| File::open(path).map_err_into(MEK::IOError))
                    .and_then(|mut file| {
                        let mut s = String::new();
                        file.read_to_string(&mut s)
                            .map(|_| s)
                            .map_err_into(MEK::IOError)
                    })
                    .map(Buffer::from)
                    .map(|buffer| Mail(reference, buffer))
            })
    }

    /// Imports mails from a directory path, recursively
    ///
    /// If a path to a file is passed, this behaves like `Mail::import_from_path()` and returns a
    /// `Vec` of len 1.
    pub fn import_from_dir<P, F, I, T: 'a>(store: &Store, p: P, tracefn: F) -> MailIterator<'a, T, I>
        where P: AsRef<Path>,
              F: Fn(&Error) -> (),
              I: Iterator<Item = Result<Mail<'a>>>
    {
        use walkdir::WalkDir;

        let iter = WalkDir::new(p)
            .follow_links(false)
            .into_iter()
            .filter_map(|result| {
                match result {
                    Err(err) => {
                        tracefn(&err);
                        None
                    },
                    Ok(entry) => if entry.file_type().is_file() {
                        Some(entry)
                    } else {
                        None
                    },
                }
            })
            .map(|entry| Mail::import_from_path(store, entry.path()));

        MailIterator::new(iter)
    }

    /// Opens a mail by the passed hash
    pub fn open<S: AsRef<str>>(store: &Store, hash: S) -> Result<Option<Mail>> {
        Ref::get_by_hash(store, String::from(hash.as_ref()))
            .map_err_into(MEK::FetchByHashError)
            .map_err_into(MEK::FetchError)
            .and_then(|o| match o {
                Some(r) => Mail::from_ref(r).map(Some),
                None => Ok(None),
            })

    }

    /// Implement me as TryFrom as soon as it is stable
    pub fn from_ref(r: Ref<'a>) -> Result<Mail> {
        r.fs_file()
            .map_err_into(MEK::RefHandlingError)
            .and_then(|path| File::open(path).map_err_into(MEK::IOError))
            .and_then(|mut file| {
                let mut s = String::new();
                file.read_to_string(&mut s)
                    .map(|_| s)
                    .map_err_into(MEK::IOError)
            })
            .map(Buffer::from)
            .map(|buffer| Mail(r, buffer))
    }

    pub fn get_field(&self, field: &str) -> Result<Option<String>> {
        use mailparse::MailHeader;

        self.1
            .parsed()
            .map_err_into(MEK::MailParsingError)
            .map(|parsed| {
                parsed.headers
                    .iter()
                    .filter(|hdr| hdr.get_key().map(|n| n == field).unwrap_or(false))
                    .next()
                    .and_then(|field| field.get_value().ok())
            })
    }

    pub fn get_from(&self) -> Result<Option<String>> {
        self.get_field("From")
    }

    pub fn get_to(&self) -> Result<Option<String>> {
        self.get_field("To")
    }

    pub fn get_subject(&self) -> Result<Option<String>> {
        self.get_field("Subject")
    }

    pub fn get_message_id(&self) -> Result<Option<String>> {
        self.get_field("Message-ID")
    }

    pub fn get_in_reply_to(&self) -> Result<Option<String>> {
        self.get_field("In-Reply-To")
    }

}

pub struct MailIterator<'a, T: 'a, I: Iterator<Item = Result<Mail<'a>>>> {
    phan: PhantomData<&'a T>,
    i: I
}

impl<'a, T: 'a, I: Iterator<Item = Result<Mail<'a>>>> MailIterator<'a, T, I> {

    pub fn new(i: I) -> MailIterator<'a, T, I> {
        MailIterator { phan: PhantomData, i: i }
    }

}

impl<'a, T: 'a, I: Iterator<Item = Result<Mail<'a>>>> Iterator for MailIterator<'a, T, I> {
    type Item = Result<Mail<'a>>;

    fn next(&mut self) -> Option<Result<Mail<'a>>> {
        self.i.next()
    }

}

