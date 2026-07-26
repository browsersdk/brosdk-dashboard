import { Fingerprint } from "lucide-react";
import type { DashboardSnapshot, EnvironmentBindingSummary } from "../../types";
import { buildFingerprintComparison, type ComparisonState } from "./fingerprintComparison";

interface FingerprintComparisonViewProps {
  environments: DashboardSnapshot["environments"];
  bindings: EnvironmentBindingSummary[];
  selectedEnvIds: string[];
}

const stateLabel: Record<ComparisonState, string> = {
  same: "相同",
  different: "不同",
  unknown: "未知",
};

export function FingerprintComparisonView({
  environments,
  bindings,
  selectedEnvIds,
}: FingerprintComparisonViewProps) {
  const comparison = buildFingerprintComparison(environments, bindings, selectedEnvIds);
  if (comparison.environments.length < 2) {
    return <div className="empty-state"><Fingerprint size={20} /><span>请选择至少两个环境</span></div>;
  }

  return (
    <div className="fingerprint-comparison">
      <div className="fingerprint-heading">
        <div><small>{comparison.environments.length}/4</small><h2>环境指纹对比</h2></div>
      </div>
      <div className="fingerprint-comparison-scroll">
        <table className="fingerprint-comparison-table">
          <thead>
            <tr>
              <th>字段</th>
              {comparison.environments.map((environment) => <th key={environment.envId} data-env-id={environment.envId} title={environment.envId}><span>{environment.name}</span><small>{environment.envId}</small></th>)}
            </tr>
          </thead>
          <tbody>
            {comparison.groups.flatMap((group) => [
              <tr className="comparison-group-row" key={`group:${group.title}`}><th colSpan={comparison.environments.length + 1}>{group.title}</th></tr>,
              ...group.rows.map((row) => (
                <tr key={`${group.title}:${row.key}`}>
                  <th><span>{row.label}</span><small className={`comparison-state ${row.state}`}>{stateLabel[row.state]}</small></th>
                  {row.values.map((value, index) => <td key={comparison.environments[index].envId} title={value}>{value}</td>)}
                </tr>
              )),
            ])}
          </tbody>
        </table>
      </div>
    </div>
  );
}
