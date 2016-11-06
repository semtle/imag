//! The linker takes an iterator of mails and links them together by their message-id, using
//! `libimaglink`.


generate_error_module!(
    generate_error_types!(LinkerError, LinkerErrorKind,
        LinkerConstructionError => "Error while build()ing the Linker object",
        NoMessageIdFoundError   => "No Message-Id for mail found",
        LinkerError             => "Error while linking mails"
    );
);

use std::collections::HashMap;
use std::fmt::{Display, Formatter, Error as FmtError, Result as FmtResult};
use std::cell::RefCell;

use libimagerror::into::IntoError;

use mail::Mail;

use self::error::LinkerError;
use self::error::LinkerErrorKind as LEK;
use self::error::MapErrInto;

bitflags! {
    pub flags LinkerOpts: u32 {
        const IGNORE_IMPORT_NOMSGID  = 0b00000001,
        const IGNORE_IMPORT_REPTOERR = 0b00000010,
        const RETURN_SOON            = 0b00000100,
        const PRINT_INFO             = 0b00001000,
    }
}

impl Display for LinkerOpts {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "{}", flags_to_str(self))
    }
}

fn flags_to_str(flgs: &LinkerOpts) -> &'static str {
    match *flgs {
        IGNORE_IMPORT_NOMSGID  => "Ignore if there was an error while fetching the Message-Id field",
        IGNORE_IMPORT_REPTOERR => "Ignore if there was an error while fetching the In-Reply-To header field",
        RETURN_SOON            => "Return as soon as an error occurs",
        PRINT_INFO             => "Print information if linking succeeded",
        LinkerOpts { .. }      => "Unknown Linker option",
    }
}

type MessageId = String;

#[derive(Debug)]
struct MemMail<'a>(Mail<'a>, Option<MessageId>);

pub struct Linker<'a> {
    v: Vec<RefCell<Mail<'a>>>,
    hm: HashMap<MessageId, Vec<MessageId>>,
    flags: LinkerOpts,
}

impl<'a> Linker<'a> {

    pub fn build<I>(i: I, flags: LinkerOpts) -> Result<Linker<'a>, LinkerError>
        where I: Iterator<Item = Mail<'a>>
    {
        use std::cmp::Ordering;

        let mut v : Vec<Mail> = i.collect();
        v.sort_by(|a, b| {
            match (a.get_message_id(), b.get_message_id()) {
                (Ok(Some(aid)), Ok(Some(bid))) => aid.cmp(&bid),
                (Ok(None), _) => Ordering::Less,
                (Err(_), _)   => Ordering::Less,
                (_, Ok(None)) => Ordering::Greater,
                (_, Err(_))   => Ordering::Greater,
            }
        });

        let mut hm : HashMap<MessageId, Vec<MessageId>> = HashMap::new();

        for mail in v.iter() {
            let m_id = match mail.get_message_id().map_err_into(LEK::LinkerConstructionError) {
                Err(e) => return Err(e),
                Ok(None) => return Err(LEK::NoMessageIdFoundError.into_error()),
                Ok(Some(mid)) => mid,
            };

            let other = try!(mail.get_in_reply_to().map_err_into(LEK::LinkerConstructionError));

            if hm.contains_key(&m_id) {
                other.map(|o| hm.get_mut(&m_id).map(|v| v.push(o)));
            } else {
                let mut to_insert = vec![];
                other.map(|o| to_insert.push(o));
                hm.insert(m_id, to_insert);
            }
        }

        Ok(Linker { v: v.into_iter().map(RefCell::new).collect(), hm: hm, flags: flags })
    }

    /// Run the linker
    ///
    /// Use the LinkerOpts `opts` to configure the linker for this run.
    ///
    /// # Return value
    ///
    /// On error, this returns a LinkerError which can then be transformed into a MailError
    ///
    pub fn run(&mut self) -> Result<(), LinkerError> {
        use libimagentrylink::internal::InternalLinker;
        use std::ops::DerefMut;

        // Naive
        fn find_in_vec<'a>(v: &Vec<RefCell<Mail<'a>>>, k: &MessageId) -> Option<RefCell<Mail<'a>>> {
            for item in v.into_iter() {
                match item.borrow().get_message_id() {
                    Ok(Some(id)) => if id == *k {
                        return Some(item.clone())
                    },
                    _ => { }, // We catch errors later...
                }
            }

            None
        }

        for (k, vs) in self.hm.iter() {
            for v in vs.iter() {
                let mut a = match find_in_vec(&mut self.v, k) { None => continue, Some(a) => a };
                let mut a = a.borrow_mut();
                let mut a = a.deref_mut();

                let mut b = match find_in_vec(&mut self.v, v) { None => continue, Some(b) => b };
                let mut b = b.borrow_mut();
                let mut b = b.deref_mut();

                try!(a.add_internal_link(b).map_err_into(LEK::LinkerConstructionError));
            }
        }

        Ok(())
    }

}

