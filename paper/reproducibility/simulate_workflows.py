#!/usr/bin/env python3
"""Finite stochastic workflow experiment with exact population moments.

Provider names, compatible effort controls and prices come from model-registry.json.
Every task, token-use, quality and duration law below is a synthetic assumption.
No API calls occur. Provider identity affects price only.
"""
from __future__ import annotations
from dataclasses import dataclass, asdict
from pathlib import Path
import argparse
import csv
import json
import platform
import time
import numpy as np
import scipy
from scipy.optimize import linprog
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.ticker import PercentFormatter

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent
SEED = 20261113
GROUPS = ['routine', 'migration']
SCENARIOS = ['nominal', 'adverse']
MU = np.array([[.70,.40],[.30,.60]])
EFFORTS = ['low','medium','high']
TOPOLOGIES = ['single','serial','parallel']
DEADLINE = 180.0
EPSILON, GAMMA, DELTA, ALPHA = .10, .80, .15, .05
INPUT = {'draft':2200,'review':3600,'repair':4200,'merge':5000}
OUTPUT = {'draft':[900,2200,5000],'review':[500,1400,3600],
          'repair':[1000,2500,5500],'merge':[700,1800,4200]}
DIFFICULT_OUTPUT = {'draft':1400,'review':800,'repair':1200,'merge':1000}

@dataclass(frozen=True)
class Candidate:
    name: str
    provider: str
    model_id: str
    topology: str
    effort: str
    cap: int
    price_in: float
    price_out: float


def candidates(registry):
    result=[]
    for model in registry['models']:
        for topology in TOPOLOGIES:
            for effort in EFFORTS:
                assert effort in model['supported_efforts']
                for cap in registry['selected_output_caps']:
                    maximum=model.get('documented_max_output_tokens')
                    assert maximum is None or cap <= maximum
                    result.append(Candidate(f"{model['model_id']}__{topology}__{effort}__{cap}",model['provider'],model['model_id'],topology,effort,cap,model['input_usd_per_million'],model['output_usd_per_million']))
    result.append(Candidate('reject','none','none','reject','none',0,0,0))
    return result


def call(candidate, role, difficulty, slowdown):
    """Return complete flag, synthetic token charge and synthetic service seconds."""
    e=EFFORTS.index(candidate.effort)
    requested=OUTPUT[role][e]+DIFFICULT_OUTPUT[role]*difficulty
    output=min(requested,candidate.cap)
    input_tokens=INPUT[role]+1000*difficulty
    dollars=(candidate.price_in*input_tokens+candidate.price_out*output)/1e6
    seconds=slowdown*(2+.008*input_tokens+.012*output)
    return requested<=candidate.cap,dollars,seconds


def branch(probability):
    return [(False,1-probability),(True,probability)]


def conditional_outcomes(candidate,difficulty,blind,scenario,slowdown):
    """Enumerate P,A,Y,C,T conditional on latent task/load values."""
    if candidate.topology=='reject':
        return [(1.,0,0,0.,0.)]
    e=EFFORTS.index(candidate.effort)
    bad=1. if blind else (.12+.26*difficulty+.06*scenario)*[1.,.62,.35][e]
    detect=.08 if blind else [.65,.82,.94][e]-.08*difficulty
    false_alarm=.03
    draft_ok,cd,td=call(candidate,'draft',difficulty,slowdown)
    if candidate.topology=='single':
        if not draft_ok: return [(1.,0,0,cd,td)]
        return [(p,1,int(is_bad),cd,td) for is_bad,p in branch(bad)]
    if candidate.topology=='serial':
        if not draft_ok:return [(1.,0,0,cd,td)]
        review_ok,cr,tr=call(candidate,'review',difficulty,slowdown)
        if not review_ok:return [(1.,0,0,cd+cr,td+tr)]
        repair_ok,cp,tp=call(candidate,'repair',difficulty,slowdown)
        outcomes=[]
        for is_bad,pdraft in branch(bad):
            flag=detect if is_bad else false_alarm
            outcomes.append((pdraft*(1-flag),1,int(is_bad),cd+cr,td+tr))
            if not repair_ok:
                outcomes.append((pdraft*flag,0,0,cd+cr+cp,td+tr+tp))
                continue
            repair_bad=1. if blind else .65*bad
            for is_repair_bad,prep in branch(repair_bad):
                final_reject=detect if is_repair_bad else false_alarm
                outcomes.append((pdraft*flag*prep*(1-final_reject),1,int(is_repair_bad),cd+2*cr+cp,td+2*tr+tp))
                outcomes.append((pdraft*flag*prep*final_reject,0,0,cd+2*cr+cp,td+2*tr+tp))
        return outcomes
    if candidate.topology=='parallel':
        # Two simultaneous drafts have equal duration conditional on the shared load.
        if not draft_ok:return [(1.,0,0,2*cd,td)]
        merge_ok,cm,tm=call(candidate,'merge',difficulty,slowdown)
        if not merge_ok:return [(1.,0,0,2*cd+cm,td+tm)]
        outcomes=[]
        for bad1,p1 in branch(bad):
            for bad2,p2 in branch(bad):
                if blind: merged_bad=1.
                elif bad1 and bad2: merged_bad=[.85,.70,.55][e]
                elif bad1 or bad2: merged_bad=[.25,.14,.07][e]+.04*difficulty
                else: merged_bad=.01+.01*difficulty
                # Aggregation also evaluates its result, using the same synthetic check law.
                for is_bad,pm in branch(merged_bad):
                    reject=detect if is_bad else false_alarm
                    outcomes.append((p1*p2*pm*(1-reject),1,int(is_bad),2*cd+cm,td+tm))
                    outcomes.append((p1*p2*pm*reject,0,0,2*cd+cm,td+tm))
        return outcomes
    raise ValueError(candidate.topology)


def distribution(candidate,group,scenario):
    result=[]
    difficult=.15+.40*group+.10*scenario
    slow_probability=.06+.08*scenario
    for d,pd in branch(difficult):
        blind_probability=.01+.02*d+.01*scenario
        for blind,pb in branch(blind_probability):
            for slow,ps in branch(slow_probability):
                for p,a,b,c,t in conditional_outcomes(candidate,int(d),blind,scenario,1.7 if slow else 1.):
                    result.append((pd*pb*ps*p,a,b,c,t,int(t>DEADLINE)))
    table=np.array(result,dtype=float)
    assert abs(table[:,0].sum()-1)<1e-12
    assert np.all(table[:,2]<=table[:,1])
    assert np.all(table[:,0]>=0)
    table[:,0]/=table[:,0].sum()
    return table


def moment_table(all_candidates):
    distributions={}
    moments=np.zeros((2,len(all_candidates),2,4))
    cmax=np.zeros((2,len(all_candidates),2))
    for g in range(2):
        for j,candidate in enumerate(all_candidates):
            for k in range(2):
                d=distribution(candidate,g,k)
                distributions[g,j,k]=d
                moments[g,j,k]=d[:,0]@d[:,[1,2,3,5]]
                cmax[g,j,k]=d[:,3].max()
    return distributions,moments,cmax


def solve(moments,allowed=None,epsilon=EPSILON,gamma=GAMMA,delta=DELTA):
    """Minimise maximum scenario expected cost with common group routing weights."""
    G,J,K,_=moments.shape
    if allowed is None:allowed=list(range(J))
    n=len(allowed)
    coeff=moments[:,allowed,:,:]
    rows=[]; rhs=[]
    for k in range(K):
        for quantity in ('cost','risk','completion','late'):
            if quantity=='cost': value=coeff[:,:,k,2]; z=-1.;bound=0.
            elif quantity=='risk':value=coeff[:,:,k,1]-epsilon*coeff[:,:,k,0];z=0.;bound=0.
            elif quantity=='completion':value=-coeff[:,:,k,0];z=0.;bound=-gamma
            else:value=coeff[:,:,k,3];z=0.;bound=delta
            rows.append(np.r_[(MU[:,k,None]*value).ravel(),z]);rhs.append(bound)
    equal=np.zeros((G,G*n+1))
    for g in range(G):equal[g,g*n:(g+1)*n]=1
    objective=np.r_[np.zeros(G*n),1.]
    result=linprog(objective,A_ub=rows,b_ub=rhs,A_eq=equal,b_eq=np.ones(G),bounds=[(0,None)]*(G*n)+[(None,None)],method='highs')
    result.allowed=allowed
    if result.success:
        weights=np.zeros((G,J));weights[:,allowed]=result.x[:-1].reshape(G,n)
        result.weights=weights
        residual=np.max(np.asarray(rows)@result.x-rhs)
        assert residual<1e-8
    return result


def metrics(weights,moments,*,proportions=MU,scenarios=SCENARIOS):
    rows=[]
    for k, scenario in enumerate(scenarios):
        a,b,c,l=np.einsum('g,gj,gjv->v',proportions[:,k],weights,moments[:,:,k,:])
        rows.append(dict(scenario=scenario,acceptance=float(a),joint_defect=float(b),risk=float(b/a) if a>0 else None,cost=float(c),late=float(l)))
    return rows


def calibration(distributions,cmax,n,seed):
    rng=np.random.default_rng(seed)
    estimates=np.zeros((*cmax.shape,4))
    N=np.count_nonzero(cmax)
    radius=np.sqrt(np.log(4*N/ALPHA)/(2*n))
    counts=[]
    for (g,j,k),table in distributions.items():
        draws=rng.multinomial(n,table[:,0])
        value=draws@table[:,[1,2,3,5]]/n
        estimates[g,j,k]=value
        counts.extend((g,j,k,h,int(count)) for h,count in enumerate(draws) if count)
    bounds=estimates.copy()
    bounds[:,:,:,0]=np.maximum(0,estimates[:,:,:,0]-radius)
    bounds[:,:,:,1]=np.minimum(1,estimates[:,:,:,1]+radius)
    bounds[:,:,:,2]=np.minimum(cmax,estimates[:,:,:,2]+cmax*radius)
    bounds[:,:,:,3]=np.minimum(1,estimates[:,:,:,3]+radius)
    known=cmax==0
    bounds[known]=estimates[known]
    return estimates,bounds,radius,counts


def write_csv(path,rows,fieldnames=None):
    rows=list(rows)
    if not rows:return
    if fieldnames is None:fieldnames=list(rows[0])
    with path.open('w',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=fieldnames)
        writer.writeheader();writer.writerows(rows)


def search(moments,all_candidates):
    allowed=[j for j,c in enumerate(all_candidates) if c.topology in ('single','reject')]
    # Feasibility seed is a fixed serial/high/8192 reference, evaluated in advance.
    seed=[j for j,c in enumerate(all_candidates) if c.provider=='Google' and c.topology=='serial' and c.effort=='high' and c.cap==8192][0]
    allowed.append(seed)
    trace=[]
    while True:
        result=solve(moments,allowed)
        if not result.success:raise RuntimeError('Search seed infeasible')
        trace.append(dict(iteration=len(trace),candidate_count=len(allowed),cost=result.fun))
        missing=[j for j in range(len(all_candidates)) if j not in allowed]
        if not missing:break
        best=(0.,None)
        # Price a whole graph column for each task group, then add it for both groups.
        for j in missing:
            for g in range(2):
                row=[]
                for k in range(2):
                    a,b,c,l=moments[g,j,k]
                    row.extend(MU[g,k]*np.array([c,b-EPSILON*a,-a,l]))
                reduced=-np.dot(result.ineqlin.marginals,row)-result.eqlin.marginals[g]
                if reduced<best[0]:best=(reduced,j)
        if best[1] is None or best[0]>-1e-10:break
        trace[-1]['added_candidate']=all_candidates[best[1]].name
        trace[-1]['reduced_cost']=best[0]
        allowed.append(best[1])
    return trace,result


def style():
    plt.rcParams.update({'font.family':'DejaVu Serif','font.size':10.5,'axes.labelsize':10.5,'axes.titlesize':11.5,
        'legend.fontsize':10.5,'xtick.labelsize':10.5,'ytick.labelsize':10.5,'pdf.fonttype':42,'ps.fonttype':42,
        'axes.spines.top':False,'axes.spines.right':False,'axes.linewidth':.6,'lines.linewidth':1.5})


def save(fig,name):
    for ext in ('pdf','png'):
        fig.savefig(PROJECT/'figures'/f'{name}.{ext}',dpi=220,bbox_inches='tight')
    plt.close(fig)


def plots(frontier,weights,all_candidates,calibration_rows):
    style(); blue='#117B93';purple='#5746B2';gray='#6C737D'
    fig,ax=plt.subplots(figsize=(6.4,3.35),layout='constrained')
    for key,col,ls,label in [('single',gray,'--','Single calls'),('full',blue,'-','All graph families')]:
        data=[x for x in frontier if x['family']==key]
        xs=[x['epsilon'] for x in data];ys=[x['cost']*1000 if x['feasible'] else np.nan for x in data]
        ax.plot(xs,ys,color=col,ls=ls,label=label)
    ax.axvline(EPSILON,color='.65',lw=.7,ls=':')
    ax.set_xlabel('Maximum defect fraction among accepted outputs')
    ax.set_ylabel('Minimum worst-scenario cost\n(USD per 1,000 attempted tasks)')
    ax.xaxis.set_major_formatter(PercentFormatter(1,decimals=0))
    ax.set_xlim(.03,.20)
    ax.set_ylim(bottom=0);ax.grid(axis='y',color='.9',lw=.5);ax.legend(frameon=False)
    ax.text(.034,12,'Full pool first feasible\nat 5% on the grid',fontsize=10.5,color='.35')
    ax.annotate('Single calls first feasible\nat 13% on the grid',xy=(.13,38.8),xytext=(.09,58),fontsize=10.5,
        arrowprops=dict(arrowstyle='-',lw=.6,color=gray),color=gray)
    save(fig,'cost-risk-frontier')

    fig,axes=plt.subplots(1,2,figsize=(6.4,3.1),layout='constrained',sharey=True)
    agg=[]
    for g,ax in enumerate(axes):
        rows=[]
        for j,c in enumerate(all_candidates):
            if weights[g,j]>1e-8:
                label='Reject' if c.topology=='reject' else f'{c.topology.capitalize()} {c.effort}\n{c.cap:,}-token cap'
                rows.append((label,weights[g,j],c.name))
        rows.sort(key=lambda x:-x[1])
        for n,(label,value,name) in enumerate(rows):
            ax.bar(n,value,color=gray if label=='Reject' else blue,width=.60)
            ax.text(n,value+.015,f'{value:.1%}',ha='center',fontsize=10.5)
            agg.append(dict(group=GROUPS[g],label=label.replace('\n',' '),weight=value,candidate=name))
        ax.set_xticks(range(len(rows)),[x[0] for x in rows],fontsize=10.5)
        ax.set_ylim(0,1.05);ax.yaxis.set_major_formatter(PercentFormatter(1,decimals=0));ax.set_title(f'({chr(97+g)}) {GROUPS[g].capitalize()} tasks',loc='left')
        ax.grid(axis='y',color='.92',lw=.5);ax.set_axisbelow(True)
    axes[0].set_ylabel('Share of attempted tasks')
    save(fig,'selected-routing')
    write_csv(HERE/'routing-plot-data.csv',agg)

    fig,axes=plt.subplots(1,2,figsize=(6.4,3.2),layout='constrained')
    xs=np.array([r['n_per_cell'] for r in calibration_rows])
    ybound=np.array([r.get('cost_bound',np.nan)*1000 for r in calibration_rows])
    ytrue=np.array([r.get('true_cost',np.nan)*1000 for r in calibration_rows])
    axes[0].plot(xs,ybound,'o-',color=purple,label='Conservative cost bound')
    axes[0].plot(xs,ytrue,'s--',color=blue,label='Exact synthetic cost')
    axes[0].set_title('(a) Worst-scenario cost',loc='left');axes[0].set_ylabel('USD per 1,000 attempts')
    axes[0].legend(frameon=False,fontsize=10.5);axes[0].set_ylim(0,60)
    risk=[r.get('true_risk',np.nan) for r in calibration_rows]
    axes[1].plot(xs,risk,'s-',color=blue)
    axes[1].axhline(EPSILON,color=gray,ls='--',label='10% defect limit')
    axes[1].yaxis.set_major_formatter(PercentFormatter(1,decimals=0))
    axes[1].set_title('(b) Worst-scenario defect risk',loc='left');axes[1].set_ylabel('Defects among accepted outputs')
    axes[1].set_ylim(0,.115);axes[1].legend(frameon=False,fontsize=10.5)
    for ax in axes:
        ax.set_xscale('log');ax.set_xlabel('Calibration trials per cell');ax.grid(axis='y',color='.9',lw=.5)
        ax.set_xticks([10000,30000,100000],['10,000','30,000','100,000'])
        ax.tick_params(axis='x',which='minor',bottom=False)
    save(fig,'calibration-cost-risk')


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--calibration-n',type=int,default=20000);args=parser.parse_args()
    started=time.perf_counter()
    registry=json.loads((HERE/'model-registry.json').read_text())
    all_candidates=candidates(registry)
    ds,moments,cmax=moment_table(all_candidates)
    assert len(all_candidates)==55
    # Identical prices imply identical coefficients, and price is the only identity effect.
    assert np.allclose(moments[:,:18,:,:],moments[:,18:36,:,:])
    assert np.allclose(moments[:,:18,:,[0,1,3]],moments[:,36:54,:,[0,1,3]])
    assert np.allclose(moments[:,36:54,:,2],.75*moments[:,:18,:,2])
    full=solve(moments)
    if not full.success:raise RuntimeError(full.message)
    initial=[j for j,c in enumerate(all_candidates) if c.topology in ('single','reject')]
    baseline=solve(moments,initial)
    trace,searched=search(moments,all_candidates)
    assert np.isclose(searched.fun,full.fun,atol=1e-10)
    estimates,bounds,radius,counts=calibration(ds,cmax,args.calibration_n,SEED+1)
    conservative=solve(bounds)
    coverage=bool(np.all(bounds[:,:,:,0]<=moments[:,:,:,0]+1e-12) and np.all(bounds[:,:,:,1:]>=moments[:,:,:,1:]-1e-12))
    rows=[]
    for n in (1000,3000,10000,30000,100000):
        est,bnd,rad,_=calibration(ds,cmax,n,SEED+n)
        sol=solve(bnd)
        row=dict(n_per_cell=n,radius=rad,feasible=bool(sol.success))
        if sol.success:
            mm=metrics(sol.weights,moments)
            row.update(cost_bound=float(sol.fun),true_cost=max(x['cost'] for x in mm),true_risk=max(x['risk'] for x in mm),true_acceptance=min(x['acceptance'] for x in mm),true_late=max(x['late'] for x in mm))
        rows.append(row)
    frontier=[]
    for eps in np.linspace(.03,.20,69):
        for label,allowed in [('single',initial),('full',None)]:
            sol=solve(moments,allowed,epsilon=float(eps))
            frontier.append(dict(family=label,epsilon=float(eps),feasible=bool(sol.success),cost=float(sol.fun) if sol.success else None))
    records=[]
    for g in range(2):
        for j,c in enumerate(all_candidates):
            for k in range(2):
                a,b,co,l=moments[g,j,k]
                records.append(dict(group=GROUPS[g],scenario=SCENARIOS[k],candidate=c.name,provider=c.provider,topology=c.topology,effort=c.effort,cap=c.cap,acceptance=a,joint_defect=b,cost_usd=co,lateness=l,cost_cap=cmax[g,j,k]))
    write_csv(HERE/'population-moments.csv',records)
    write_csv(HERE/'frontier.csv',frontier)
    write_csv(HERE/'calibration-size.csv',rows,fieldnames=['n_per_cell','radius','feasible','cost_bound','true_cost','true_risk','true_acceptance','true_late'])
    write_csv(HERE/'search-trace.csv',trace,fieldnames=['iteration','candidate_count','cost','added_candidate','reduced_cost'])
    write_csv(HERE/'calibration-counts.csv',(dict(group=g,candidate=j,scenario=k,outcome=h,count=n) for g,j,k,h,n in counts))
    write_csv(HERE/'finite-outcomes.csv',(dict(group=g,candidate=j,scenario=k,outcome=h,probability=p,acceptance=a,joint_defect=b,cost_usd=c,duration_seconds=t,lateness=l) for (g,j,k),table in ds.items() for h,(p,a,b,c,t,l) in enumerate(table)))
    calibration_records=[]
    for g,j,k in ds:
        record=dict(group=GROUPS[g],candidate=all_candidates[j].name,scenario=SCENARIOS[k])
        for h,name in enumerate(['acceptance','joint_defect','cost_usd','lateness']):
            record['estimate_'+name]=estimates[g,j,k,h]
            record['bound_'+name]=bounds[g,j,k,h]
        calibration_records.append(record)
    write_csv(HERE/'calibration-moments.csv',calibration_records)
    with (HERE/'candidate-configurations.json').open('w') as f:json.dump([asdict(c) for c in all_candidates],f,indent=2)
    result={'evidence_status':'Synthetic finite stochastic experiment; no LLM API calls','seed':SEED,'candidate_count':len(all_candidates),'groups':GROUPS,'scenarios':SCENARIOS,'group_proportions':MU.tolist(),'deadline_seconds':DEADLINE,'limits':dict(completion=GAMMA,defects=EPSILON,lateness=DELTA),'known_coefficient_optimum':dict(cost=float(full.fun),metrics=metrics(full.weights,moments),weights=full.weights.tolist()),'single_only':dict(feasible=bool(baseline.success),cost=float(baseline.fun) if baseline.success else None),'search':dict(iterations=len(trace),final_candidates=len(searched.allowed),cost=float(searched.fun),scope='Exact pricing across the finite known-coefficient pool'), 'calibration':dict(n_per_cell=args.calibration_n,family_alpha=ALPHA,radius=radius,all_bounds_cover=coverage,feasible=bool(conservative.success)), 'versions':dict(python=platform.python_version(),numpy=np.__version__,scipy=scipy.__version__,matplotlib=matplotlib.__version__)}
    if conservative.success:result['calibration'].update(cost_bound=float(conservative.fun),true_metrics=metrics(conservative.weights,moments),weights=conservative.weights.tolist())
    plots(frontier,full.weights,all_candidates,rows)
    result['elapsed_seconds']=time.perf_counter()-started
    (HERE/'simulation-results.json').write_text(json.dumps(result,indent=2))
    print(json.dumps({k:v for k,v in result.items() if k not in ('known_coefficient_optimum','calibration')},indent=2))
    print('Known optimum',full.fun,metrics(full.weights,moments))
    print('Calibration',result['calibration'])
    print('Sizes',rows)

if __name__=='__main__':main()
