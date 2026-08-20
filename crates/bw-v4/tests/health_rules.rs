//! 健康灯的判定规则。这几条是产品铁律,不是可选行为:
//! **没数据是灰,不是绿,也不是红。**

use bw_v4::derive::{derive_project_health, HealthInputs};
use bw_v4::model::Signal;

fn all_good() -> HealthInputs {
    HealthInputs {
        has_week_goal: true,
        committed_this_week: true,
        committed_last_week: true,
        git_readable: true,
        has_metric_reading: true,
        any_metric_red: false,
        delivered_last_week: true,
    }
}

#[test]
fn no_data_at_all_is_grey_not_red() {
    // 刚接入、还没配工作区的项目:git 根本读不动,零提交说明不了任何事。
    let s = derive_project_health(&HealthInputs::empty()).signal();
    assert_eq!(
        s,
        Signal::Unknown,
        "没数据必须是灰;红是在替用户断言项目停了"
    );
}

#[test]
fn no_data_at_all_is_not_green() {
    assert_ne!(
        derive_project_health(&HealthInputs::empty()).signal(),
        Signal::Green
    );
}

#[test]
fn two_silent_weeks_in_a_readable_repo_is_red() {
    let inputs = HealthInputs {
        git_readable: true,
        ..HealthInputs::empty()
    };
    assert_eq!(derive_project_health(&inputs).signal(), Signal::Red);
}

#[test]
fn a_red_metric_beats_three_green_judgements() {
    let inputs = HealthInputs {
        any_metric_red: true,
        ..all_good()
    };
    assert_eq!(
        derive_project_health(&inputs).signal(),
        Signal::Red,
        "指标越线不能被「三条判据都齐了」吞掉"
    );
}

#[test]
fn three_judgements_all_true_is_green() {
    assert_eq!(derive_project_health(&all_good()).signal(), Signal::Green);
}
