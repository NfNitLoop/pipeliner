Changelog
=========

v1.0.0 - November 18, 2018
--------------------------

 * Support for ordered mapping. Use `ordered_map()` if you need the order of
   your outputs to match the order of their inputs. This adds a performance
   penalty for the extra bookkeeping required. And you may see additional
   performance loss due to head-of-line blocking, depending on your workload(s).
 * Switched implementation to use crossbeam_channel, which greatly
   [improves performance][1].
 * Shrank the API. `map()` and `ordered_map()` return `impl Iterator`, so we
   no longer leak the internal `PipelineIter` type(s).

[1]: https://github.com/NfNitLoop/pipeliner/commit/c8b23a04242d6eac91df424022f62a3074c31eb0


v0.1.1 - December 5, 2016
-------------------------

Initial release. 

 * Used std::sync::mpsc
