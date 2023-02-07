use std::fmt::Display;

use async_trait::async_trait;
use data_types::{ParquetFile, PartitionId};
use metric::{Registry, U64Counter};

use crate::error::DynError;

use super::PartitionFilter;

#[derive(Debug)]
pub struct MetricsPartitionFilterWrapper<T>
where
    T: PartitionFilter,
{
    pass_counter: U64Counter,
    filter_counter: U64Counter,
    error_counter: U64Counter,
    inner: T,
    filter_type: &'static str,
}

impl<T> MetricsPartitionFilterWrapper<T>
where
    T: PartitionFilter,
{
    pub fn new(inner: T, registry: &Registry, filter_type: &'static str) -> Self {
        let metric = registry.register_metric::<U64Counter>(
            "iox_compactor_partition_filter_count",
            "Number of times the compactor fetched fresh partitions",
        );

        let pass_counter = metric.recorder(&[(filter_type, "pass")]);
        let filter_counter = metric.recorder(&[(filter_type, "filter")]);
        let error_counter = metric.recorder(&[(filter_type, "error")]);

        Self {
            pass_counter,
            filter_counter,
            error_counter,
            inner,
            filter_type,
        }
    }
}

impl<T> Display for MetricsPartitionFilterWrapper<T>
where
    T: PartitionFilter,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "metrics({}, {})", self.inner, self.filter_type)
    }
}

#[async_trait]
impl<T> PartitionFilter for MetricsPartitionFilterWrapper<T>
where
    T: PartitionFilter,
{
    async fn apply(
        &self,
        partition_id: PartitionId,
        files: &[ParquetFile],
    ) -> Result<bool, DynError> {
        let res = self.inner.apply(partition_id, files).await;
        match res {
            Ok(true) => {
                self.pass_counter.inc(1);
            }
            Ok(false) => {
                self.filter_counter.inc(1);
            }
            Err(_) => {
                self.error_counter.inc(1);
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use metric::{Attributes, Metric};

    use crate::{
        components::partition_filter::has_files::HasFilesPartitionFilter,
        test_util::ParquetFileBuilder,
    };

    use super::*;

    #[test]
    fn test_display() {
        let registry = Registry::new();
        let filter =
            MetricsPartitionFilterWrapper::new(HasFilesPartitionFilter::new(), &registry, "test");
        assert_eq!(filter.to_string(), "metrics(has_files, test)",);
    }

    #[tokio::test]
    async fn test_apply() {
        let registry = Registry::new();
        let filter =
            MetricsPartitionFilterWrapper::new(HasFilesPartitionFilter::new(), &registry, "test");
        let p_id = PartitionId::new(1);
        let f = ParquetFileBuilder::new(0).build();

        assert_eq!(pass_counter(&registry), 0);
        assert_eq!(filter_counter(&registry), 0);
        assert_eq!(error_counter(&registry), 0);

        assert!(!filter.apply(p_id, &[]).await.unwrap());
        assert!(!filter.apply(p_id, &[]).await.unwrap());
        assert!(filter.apply(p_id, &[f]).await.unwrap());

        assert_eq!(pass_counter(&registry), 1);
        assert_eq!(filter_counter(&registry), 2);
        assert_eq!(error_counter(&registry), 0);
    }

    fn pass_counter(registry: &Registry) -> u64 {
        registry
            .get_instrument::<Metric<U64Counter>>("iox_compactor_partition_filter_count")
            .expect("instrument not found")
            .get_observer(&Attributes::from(&[("test", "pass")]))
            .expect("observer not found")
            .fetch()
    }

    fn filter_counter(registry: &Registry) -> u64 {
        registry
            .get_instrument::<Metric<U64Counter>>("iox_compactor_partition_filter_count")
            .expect("instrument not found")
            .get_observer(&Attributes::from(&[("test", "filter")]))
            .expect("observer not found")
            .fetch()
    }

    fn error_counter(registry: &Registry) -> u64 {
        registry
            .get_instrument::<Metric<U64Counter>>("iox_compactor_partition_filter_count")
            .expect("instrument not found")
            .get_observer(&Attributes::from(&[("test", "error")]))
            .expect("observer not found")
            .fetch()
    }
}
