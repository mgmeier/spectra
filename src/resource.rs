// FIXME: add the support of transient objects

use notify::{self, RecommendedWatcher, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use time::precise_time_s;

use id::Id;
use model::Model;
use object::Object;
use shader::Program;

/// Class of types that can be loaded.
pub trait Load<'a>: Sized {
  /// Arguments passed at loading.
  type Args;

  // TODO: see whether we can use something with From/Into instead, so that we can use lambdas.
  fn load<P>(path: P, cache: &mut Cache<'a>, args: Self::Args) -> Result<Self, LoadError> where P: AsRef<Path>;
}

/// Class of types that can be reloaded.
///
/// The idea is to simply recover the arguments used in `Load::load`.
pub trait Reload<'a>: Load<'a> {
  fn reload_args(&self) -> Self::Args;
}

/// Default implementation for types which are loaded without any arguments.
impl<'a, T> Reload<'a> for T where T: Load<'a, Args=()> {
  fn reload_args(&self) -> Self::Args {
    ()
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadError {
  FileNotFound(PathBuf, String),
  ParseFailed(String),
  ConversionFailed(String)
}

type Timestamp = f64;

/// Time to await after a resource update to establish that it should be reloaded.
const UPDATE_AWAIT_TIME: Timestamp = 0.1; // 100ms

struct CacheBlock<'a, T> where T: 'a {
  data: Vec<(T, PathBuf, (Receiver<Timestamp>, f64))>,
  ids: HashMap<String, Id<'a, T>>,
}

impl<'a, T> CacheBlock<'a, T> {
  pub fn new() -> Self {
    CacheBlock {
      data: Vec::new(),
      ids: HashMap::new(),
    }
  }
}

macro_rules! cache_struct {
  ($l:tt, $($n:ident : $t:ty),*) => {
    pub struct Cache<$l> {
      senders: Arc<Mutex<HashMap<PathBuf, Sender<Timestamp>>>>,
      $(
        $n: CacheBlock<$l, $t>
      ),*
    }

    impl<$l> Cache<$l> {
      pub fn new<P>(root:P) -> Self where P: AsRef<Path> {
        let senders: Arc<Mutex<HashMap<PathBuf, Sender<Timestamp>>>> = Arc::new(Mutex::new(HashMap::new()));

        // start watcher thread
        {
          let senders = senders.clone();
          let root = root.as_ref().to_path_buf();
          let (wsx, wrx) = channel();
          let mut watcher: RecommendedWatcher = Watcher::new(wsx).unwrap();

          let _ = thread::spawn(move || {
            let _ = watcher.watch(root);

            for event in wrx.iter() {
              match event {
                notify::Event { path: Some(path), op: Ok(notify::op::WRITE) } => {
                  if let Some(sx) = senders.lock().unwrap().get(&path) {
                    sx.send(precise_time_s()).unwrap();
                  }
                },
                _ => {}
              }
            }
          });
        }

        Cache {
          senders: senders,
          $(
            $n: CacheBlock::new()
          ),*
        }
      }
    }
  }
}

pub trait Get<'a, T> where T: 'a + Reload<'a> {
  fn get_id(&mut self, name: &str, args: T::Args) -> Option<Id<'a, T>>;
  fn get_by_id(&mut self, id: &Id<'a, T>) -> Option<&T>;
  fn get(&mut self, name: &str, args: T::Args) -> Option<&T> {
    self.get_id(name, args).and_then(move |i| self.get_by_id(&i))
  }
}

macro_rules! impl_get_id {
  ($n:ident : $t:ty, $this:ident, $name:ident, $args: ident) => {{
    let path_str = format!("data/{}/{}", stringify!($n), $name);
    let path = Path::new(&path_str);

    match $this.$n.ids.get($name).cloned() {
      id@Some(..) => {
        deb!("cache hit for {}", path_str);
        id
      },
      None => {
        deb!("cache miss for {}", path_str);

        // specific loading
        if path.exists() {
          match <$t as Load>::load(&path, $this, $args) {
            Ok(resource) => {
              let path_buf = path.to_owned();

              // create the id if we have loaded the resource
              let id: Id<$t> = ($this.$n.data.len() as u32).into();

              // create a channel to notify any update later and register the sender for the
              // given path
              let (sx, rx) = channel();
              {
                let mut senders = $this.senders.lock().unwrap();
                senders.insert(path_buf.clone(), sx);
              }

              // add the resource to the list of loaded ones
              $this.$n.data.push((resource, path_buf.clone(), (rx, precise_time_s())));
              // cache the resource
              $this.$n.ids.insert($name.to_owned(), id.clone());

              Some(id)
            },
            Err(e) => {
              err!("unable to load resource from {}: {:?}", path_str, e);
              None
            }
          }
        } else { // path doesn’t exist
          err!("ressource at {} cannot be found", path_str);
          None
        }
      }
    }
  }}
}

macro_rules! impl_get_by_id {
  ($n:ident : $t:ty, $this:ident, $id:ident) => {{
    // synchronization
    let mut reload_args = None;

    if let Some(data) = $this.$n.data.get($id.id as usize) {
      match (data.2).0.try_recv() {
        Ok(timestamp) if timestamp - (data.2).1 >= UPDATE_AWAIT_TIME => {
          reload_args = Some((data.1.to_owned(), data.0.reload_args()));
        },
        _ => {}
      }
    } else {
      return None;
    }

    if let Some((path, args)) = reload_args {
      match <$t as Load>::load(&path, $this, args) {
        Ok(new_resource) => {
          // replace the current resource with the freshly loaded one
          deb!("reloaded resource from {:?}", path);
          $this.$n.data[$id.id as usize].0 = new_resource;
        },
        Err(e) => {
          warn!("reloading resource from {:?} has failed: {:?}", path, e);
        }
      }
    }

    $this.$n.data.get($id.id as usize).map(|r| &r.0)
  }}
}

cache_struct!('a,
              models: Model,
              objects: Object<'a>,
              shader_programs: Program);

impl<'a> Get<'a, Model> for Cache<'a> {
  fn get_id(&mut self, name: &str, args: <Model as Load<'a>>::Args) -> Option<Id<'a, Model>> {
    impl_get_id!(models: Model, self, name, args)
  }

  fn get_by_id(&mut self, id: &Id<'a, Model>) -> Option<&Model> {
    impl_get_by_id!(models: Model, self, id)
  }
}

