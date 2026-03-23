Title: MemFly: On-the-Fly Memory Optimization via Information Bottleneck

URL Source: https://arxiv.org/pdf/2602.07885

Published Time: Tue, 10 Feb 2026 02:09:26 GMT

Number of Pages: 14

Markdown Content:
# MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

Zhenyuan Zhang 1 * Xianzhang Jia 1 * Zhiqin Yang 1 Zhenbo Song 2 Wei Xue 1 Sirui Han 1 Yike Guo 1

## Abstract 

Long-term memory enables large language model agents to tackle complex tasks through histori-cal interactions. However, existing frameworks encounter a fundamental dilemma between com-pressing redundant information efficiently and maintaining precise retrieval for downstream tasks. To bridge this gap, we propose MEM FLY ,a framework grounded in information bottleneck principles that facilitates on-the-fly memory evo-lution for LLMs. Our approach minimizes com-pression entropy while maximizing relevance en-tropy via a gradient-free optimizer, constructing a stratified memory structure for efficient storage. To fully leverage M EM FLY , we develop a hybrid retrieval mechanism that seamlessly integrates semantic, symbolic, and topological pathways, incorporating iterative refinement to handle com-plex multi-hop queries. Comprehensive experi-ments demonstrate that M EM FLY substantially outperforms state-of-the-art baselines in memory coherence, response fidelity, and accuracy. 

## 1. Introduction 

The evolution of Large Language Models (LLMs) from stateless reasoning engines to persistent autonomous agents necessitates robust long-term memory systems capable of supporting complex, extended reasoning tasks (Xi et al., 2023; Wang et al., 2024; Ferrag et al., 2025). Such memory systems must address fundamental challenges: retaining entity states that evolve over time, resolving temporal de-pendencies across interaction sessions, and synthesizing evidence distributed across numerous conversational turns. However, existing frameworks encounter a fundamental dilemma between compressing redundant information ef-ficiently and maintaining precise retrieval for downstream tasks. Existing memory frameworks (Shinn et al., 2023; Sumers et al., 2024; Zhang et al., 2025; Fang et al., 2025; Zhai   

> *Equal contribution 1The Hong Kong University of Science and Technology 2Nanjing University of Science and Technology.
> Preprint. February 10, 2026.

et al., 2025) generally fall into two paradigms, neither of which adequately resolves this tension. Retrieval-centric approaches (Lewis et al., 2021; Asai et al., 2023; Yan et al., 2024; Gao et al., 2024; Ram et al., 2023) preserve verbatim details but accumulate redundancy without consolidation, leading to monotonic entropy increase and elevated retrieval noise. Memory-augmented approaches (Packer et al., 2024; Zhong et al., 2023; Xu et al., 2025; Wang et al., 2025) employ LLM-driven summarization for compression but sacrifice fine-grained fidelity required for precise reason-ing. Both paradigms lack a unified, principled objective for determining what information to retain versus discard. This challenge is fundamentally an information-theoretic optimization problem that aligns with the Information Bot-tleneck (IB) principle (Slonim & Tishby, 1999): compress redundant observations while preserving sufficient fidelity for future tasks. To bridge this gap, we propose M EM FLY (Mem ory opti-mization on-the-Fly ), a framework grounded in information bottleneck principles that facilitates on-the-fly memory evo-lution for LLMs. Building upon the Agglomerative Informa-tion Bottleneck algorithm (Slonim & Tishby, 1999), M EM -FLY addresses the compression-fidelity trade-off through two complementary mechanisms. To construct memory, we employ an LLM-driven gradient-free optimizer, which approximates Jensen-Shannon divergence through semantic assessment and actively merges redundant content to mini-mize representational complexity I(X; M ) during memory ingestion. Simultaneously, we maintain a stratified Note-Keyword-Topic hierarchy grounded in the double clustering principle (Slonim & Tishby, 2000), where Keywords serve as intermediate symbolic anchors stabilizing the semantic space between raw observations (Notes) and high-level se-mantic regions (Topics), thereby preserving task-relevant information I(M ; Y ).To leverage constructed memory, we design a hybrid re-trieval mechanism that seamlessly integrates semantic, sym-bolic, and topological pathways: macro-semantic navigation through Topics, micro-symbolic anchoring through Key-words, and topological expansion through associative links established during consolidation. For complex queries re-quiring multi-hop reasoning, we further introduce an iter-ative refinement protocol that progressively expands the evidence pool until sufficient information is gathered. The 1

> arXiv:2602.07885v1 [cs.AI] 8 Feb 2026 MemFly: On-the-Fly Memory Optimization via Information Bottleneck

contributions of this work are summarized as follows: • We formalize agentic memory as an Online Informa-tion Bottleneck problem, unifying the treatment of entropy accumulation and fidelity loss within a single theoretical framework. • We propose two mechanisms to optimize this objective: a gradient-free optimizer that extends AIB to online settings through LLM-based semantic assessment, and a Note-Keyword-Topic hierarchy grounded in double clustering that preserves evidence structure. • We design tri-pathway retrieval with iterative refine-ment to exploit the optimized structure for complex reasoning tasks. • Extensive evaluations on comprehensive benchmarks demonstrate that M EM FLY achieves substantial im-provements, significantly outperforming state-of-the-art baselines. 

## 2. Related Work 

2.1. Retrieval-Centric Systems 

Retrieval-augmented generation (RAG) (Lewis et al., 2021; Gao et al., 2024) has evolved from passive retrieve-then-read pipelines to active, iterative workflows. Recent ad-vances introduce inference-time feedback loops for query refinement and hallucination filtering (Asai et al., 2023; Yan et al., 2024). Structural approaches further organize knowledge into graphs, enabling both local retrieval and global summarization (Edge et al., 2025; Wu et al., 2025). Beyond document-level retrieval, graph-based memory sys-tems such as MemWalker (Chen et al., 2023) maintain struc-tured knowledge representations through explicit traversal mechanisms. Despite these sophisticated capabilities, such methods fun-damentally operate as inference-time optimizations that re-fine the read path for specific queries while treating the underlying memory structure as a passive index. Conse-quently, these systems rely on query-centric embedding similarity to initiate retrieval, rendering them vulnerable to vector dilution in scenarios requiring multi-hop evidence synthesis. 

2.2. Memory-Augmented Agents 

While retrieval-centric systems optimize retrieval, an orthog-onal research direction addresses the construction path: how to structure and compress interaction history for effective long-term retention. Systems like MemGPT (Packer et al., 2024) and HiAgent (Hu et al., 2024) orchestrate context through tiered storage hierarchies, swapping information between active working memory and archival storage to emulate infinite retention. Parallel efforts seek to replicate biological memory pro-cesses. MemoryBank (Zhong et al., 2023) incorporates the Ebbinghaus forgetting curve to modulate information decay. A-MEM (Xu et al., 2025) and O-Mem (Wang et al., 2025) adopt associative strategies to foster autonomous knowledge evolution, such as, Zettelkasten-style linking or user-centric profiling. These approaches effectively mitigate the Gold-fish Effect, the tendency of LLMs to prioritize recent context while losing track of earlier information (Hans et al., 2024), by structuring interaction history into discrete, retrievable memory units. While effective for managing token bud-gets, these approaches optimize for compression efficiency without a principled mechanism for preserving task-relevant information. 

## 3. The M EM FLY Framework 

We formulate the construction of agentic long-term memory as an Information Bottleneck (IB) optimization problem. In this framework, the memory system is not a static repos-itory but a dynamic channel that compresses continuous input streams into a compact, relevance-maximizing rep-resentation. Figure 1 illustrates the overall architecture of MEM FLY .

3.1. Problem Formulation Notation. Let X1: t = {x1, x 2, . . . , x t} denote a continu-ous stream of interaction data observed by the agent, where 

xt ∈ D represents the input at time t. We define the agent’s memory state at time t as a random variable Mt taking val-ues in a structured state space. For computational realization, we instantiate Mt as a dynamic graph Gt = ( Vt, Et, Φt),where Vt is the set of memory nodes, Et ⊆ V t × V t rep-resents topological connections, and Φt : Vt → Rd × Σ∗

maps each node to its dense embedding and textual content, with Σ∗ denoting the set of all strings over alphabet Σ.

Remark 3.1 (Generality of the Framework) . While we em-ploy a graph-based realization for its computational con-venience in supporting discrete merge, link, and query op-erations, the underlying IB formulation is agnostic to the specific storage medium. Alternative instantiations, such as hierarchical databases or hybrid neuro-symbolic structures, are equally valid carriers for the abstract memory state Mt.

The Optimization Objective. Following the Informa-tion Bottleneck principle (Slonim & Tishby, 1999; Tishby & Zaslavsky, 2015), our goal is to learn a memory con-struction policy that maps the observed interaction history 

X = {x1, . . . , x t} to the memory state Mt that maximizes task-relevant information while minimizing representational complexity. This is formalized as minimizing the Memory 2MemFly: On-the-Fly Memory Optimization via Information Bottleneck Memory Construction 

(§3.3)  

> Input Str eam ��:�

…                   

> �1�2��. . . ,
> Semantic Ingestion Real-time
> LLM
> Optimizer
> ��
> ��(raw) ��(context)
> ��(keywords) ℎ�(embedding)
> Link Append Merge
> �red>���comp>��otherwise
> Minimize
> �(�;�)
> Preserve
> �(�;�)
> Δ�=Δ�(�;�)
> −β∙Δ�(�;�)

Memory State 

(Three-Layer Hierarchy)                   

> Topics �(Navigate Layer)
> ��
> �={�1,�2, . . . , ��}
> ���_�����
> ��
> ��
> ��
> ℵ={�1,�2, . . . } Information Bottleneck :
> min �(�;�)
> −β∙Δ�(�;�)
> Keywords �(Anchoring Layer)
> Notes ℵ(Fidelity Layer)
> �Hierarchy
> �Hierarchy
> Double Clustering Principle

Memory Retrieval 

(§3.4)                                  

> Query �
> Semantic
> Parser ℱ�
> ℎtopic
> �keys
> ={ℎ�1, . . . }
> Tri-Pathway Retrieval
> Macro-Semantic (via Topics)
> Micro-Symbolic (via Keywords)
> Topological (via ��������)
> ��������
> �expand= { �|( �,�) ∈ ��������}
> �∗=TopK (��� (ℎ�,��))
> �∗→ℵ
> �∗=TopK (��� (ℎ�����,��))
> �∗→�→ℵ
> 1
> 2
> 3
> RRF Fusion
> �pool
> Iterative Evidence Refinement (IRE)
> �(�)→Sufficient ?→�(�+1)→�(�)
> �=1,2, . . . , ����Answer

Figure 1. Overview of the M EM FLY framework. Left : Memory construction processes incoming observations through semantic ingestion and gated structural update, where an LLM-based optimizer performs Merge, Link, or Append operations to minimize the IB objective. 

Center : The memory state is organized as a stratified Note-Keyword-Topic hierarchy with associative edges following the double clustering principle. Right : Memory retrieval employs tri-pathway search via Topics, Keywords, and topological expansion, followed by iterative evidence refinement for complex queries. 

Information Bottleneck Lagrangian LIB :

min  

> π

LIB (Mt) = I(X1: t; Mt)

| {z }

> Compression

−β I (Mt; Y )

| {z }

> Relevance

, (1) where π denotes the memory construction policy, β > 0

controls the compression-relevance trade-off, and Y repre-sents future reasoning tasks. The Compression term I(X1: t; Mt) measures how much information from the raw input stream is retained in the memory state. Minimizing this term encourages the system to merge redundant information and discard irrelevant de-tails. The Relevance term I(Mt; Y ) measures the mutual information between the memory state and future tasks Y .Maximizing this term ensures retention of critical evidence for downstream reasoning. A key challenge in applying the Information Bottleneck prin-ciple to agentic memory is that future tasks Y are unknown at construction time. We define the relevance variable Y

as the latent semantic structure governing future reasoning tasks. Since Y is not directly observable during memory construction, we approximate it through two proxy signals: (1) local coherence: the semantic consistency within and across memory units, captured by Keyword co-occurrence patterns; (2) global navigability: the accessibility of evi-dence chains, captured by the Topic hierarchy and associa-tive links. These proxies reflect the observation that rea-soning tasks typically require either entity-centric evidence retrieval or thematic evidence aggregation. Our ablation study (Sec. 4.3) empirically validates that optimizing these structural surrogates significantly improves downstream re-sponse fidelity and accuracy. 

Online Approximation via Greedy Agglomeration. Di-rectly optimizing Eq. (1) over the entire history is compu-tationally intractable due to the combinatorial explosion of possible memory configurations. Following the Agglom-erative Information Bottleneck (AIB) algorithm (Slonim & Tishby, 1999), we adopt an online greedy strategy that makes locally optimal decisions at each time step. Specifically, we model the memory evolution as an on-line decision process where the state transition Mt+1 ←T (Mt, x t) is governed by a policy π. At each step, the policy seeks to minimize the incremental Lagrangian cost: 

∆L = I(X1: t+1 ; Mt+1 ) − I(X1: t; Mt)

| {z }

> ∆Icompress

− β �I(Mt+1 ; Y ) − I(Mt; Y )| {z }

> ∆Irelevance

. (2) In the original AIB algorithm, the merge decision between clusters zi and zj is determined by minimizing the informa-3MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

tion loss quantified via the Jensen-Shannon divergence: 

δI Y (zi, z j ) = 

�p(zi) + p(zj ) · DJS 

p(Y |zi), p (Y |zj ). (3) 

LLM as JS-Divergence Approximator. Computing Eq. (3) exactly requires access to the conditional distri-butions p(Y |zi) and p(Y |zj ), which are unavailable since future tasks Y are unknown. We address this through a key observation: JS-divergence measures distributional similar-ity, which correlates with semantic similarity assessable by LLMs pre-trained on diverse tasks. Formally, we employ an LLM as a gradient-free (Yang et al., 2024) policy π(Mt, x t) that approximates merge decisions through semantic assessment. Given two memory units nt

and ni, the LLM evaluates their relationship and outputs scores sred (nt, n i) and scomp (nt, n i) defined in Sec. 3.3.2. We hypothesize that redundancy scores are inversely related to JS-divergence: 

sred (nt, n i) ≈ 1 − DJS 

p(Y |nt), p (Y |ni), (4) where high redundancy indicates low JS-divergence, sug-gesting the units would provide similar information for downstream tasks. This design choice leverages the LLM’s implicit knowledge of task-relevant distributional properties acquired during pre-training, and is empirically validated in our ablation study (Sec. 4.3). 

3.2. Structural Prior 

To ensure the computational tractability of the online op-timization, we impose a structural prior on the memory state Mt. Direct manipulation of high-dimensional embed-ding spaces is ill-posed due to the curse of dimensionality, which manifests as sparsity and noise in similarity struc-tures (Slonim & Tishby, 2000). To mitigate these topolog-ical degradations, we draw upon the design rationale of the Double Clustering framework established by Slonim and Tishby (Slonim & Tishby, 2000). Their information-theoretic analysis demonstrated that for high-dimensional co-occurrence data, optimal compression is achieved not by clustering data points directly, but by first clustering the feature space to form robust intermediate representations. Specifically, the framework posits a two-stage abstraction process: words are first aggregated into ”word clusters” (Y → ˜Y ) based on their conditional distributions p(x|y),yielding distributionally robust feature centroids. Subse-quently, documents are clustered ( X → ˜X) based on their distributions over these word clusters p(˜ y|x). This interme-diate symbolic layer resolves the sparsity issue, allowing the system to achieve superior structural organization by projecting data onto a denser, less noisy representation. Adhering to this principle, M EM FLY instantiates the mem-ory state as a stratified Note-Keyword-Topic hierarchy: 

Layer 1: Notes N (Fidelity Layer). At the atomic level, we maintain the set of Notes, N = {n1, . . . , n N }, serving as non-parametric memory units. Formally, each note is defined as a tuple ni = ( ri, c i, hi, Ki), where ri denotes the raw observational data (verbatim content) and ci rep-resents the augmented context—a semantically denoised summary generated to enhance retrieval relevance. To facil-itate hybrid access, these textual components are mapped into dual representational spaces: a continuous dense em-bedding hi ∈ Rd encoding the context ci, and a discrete set of symbolic keywords Ki ⊂ K serving as topological an-chors. Analogous to the input variable X in the Information Bottleneck framework, this layer is designed to preserve raw observational fidelity, mathematically approximating the condition I(N ; X) ≈ H(X). By explicitly maintaining non-parametric access to original inputs, we effectively mit-igate the hallucination risks inherent in purely parametric or compression-heavy memory systems. 

Layer 2: Keywords K (Anchoring Layer). To bridge continuous embedding spaces and discrete symbolic reason-ing, we introduce Keywords K = {k1, . . . , k K } as interme-diate symbolic anchors. This layer serves an analogous role to word clusters ( ˜Y ) in the double clustering framework. Unlike the original double clustering approach, which de-rives word clusters from co-occurrence statistics p(x|y),MEM FLY extracts Keywords via LLM-based semantic pars-ing during the ingestion phase (Sec. 3.3.1). While this substitutes distributional clustering with neural semantic extraction, the functional role remains identical: Keywords provide a lower-dimensional, distributionally robust feature space that stabilizes semantic proximity and mitigates vec-tor dilution. The quality of this extraction depends on the LLM’s capability, which we optimize through task-specific prompting strategies. Keywords resolve semantic sparsity by grounding proxim-ity in shared symbolic substructures rather than potentially spurious vector correlations. Each Keyword kj maintains its own embedding ej ∈ Rd and tracks co-occurrence re-lationships with other Keywords extracted from the same Notes, forming the edge set ECO OCCUR .

Layer 3: Topics T (Navigation Layer). At the macro level, we aggregate keywords into topics T =

{C1, . . . , C T } based on their co-occurrence structure, anal-ogous to document clusters ( ˜X) in the double clustering framework. Topics serve as semantic centroids that partition the memory latent into navigable regions, enabling O(1) 

macro-semantic localization during retrieval. 4MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

3.3. Memory Construction 

To tractably minimize the Memory IB Lagrangian (Eq. (1) ), MEM FLY employs a computation-on-construction mecha-nism. We model the memory update as an online agglom-erative process, comprising three stages: ingestion, gated structural update, and topic evolution. 3.3.1. S EMANTIC INGESTION AND DENOISING .Raw input streams often contain elliptical references, syn-tactic noise, and implicit context. We project raw input xt

into a structured Note nt via an LLM-based transformation: 

nt = Fingest (xt) = ( rt, c t, ht, Kt), (5) where rt preserves raw content, ct is the denoised context, 

ht = Embed (ct) ∈ Rd, and Kt ⊆ K is the extracted Key-word set. This transformation enhances signal-to-noise ratio, improving I(nt; Y ) relative to I(xt; Y ).3.3.2. G ATED STRUCTURAL UPDATE 

Before consolidation, we retrieve a candidate neighborhood 

Ncand by querying existing memory through dual sparse-dense indices, localizing the decision space to the most relevant subgraph. The LLM policy evaluates each candidate pair (nt, n i) with 

ni ∈ N cand by generating two scalar scores through struc-tured prompting: a redundancy score sred (nt, n i) ∈ [0 , 1] 

and a complementarity score scomp (nt, n i) ∈ [0 , 1] . Specif-ically, sred quantifies the semantic overlap between units, where a unit value indicates identity in informational con-tent. Conversely, scomp measures the strength of logical or topical connections between nodes that possess distinct, non-overlapping information. The prompting templates are provided in Appendix. 

Structural Operations. Based on these scores, the policy executes one of three operations: 

Mt+1 ← O (nt, n i) = 



MERGE (ni ← ni ⊕ nt) if sred (nt, n i) > τ m

LINK (ni ↔ nt) if scomp (nt, n i) > τ l

APPEND (nt) otherwise 

, (6) where τm and τl are threshold hyperparameters. 

Merge Operation. When sred > τ m, the content of nt is integrated into ni:

r′ 

> i

= ri ∪ rt, (7) 

c′ 

> i

= Fmerge (ci, c t), (8) 

ni ← (r′

> i

, c ′

> i

, Embed (c′

> i

), Ki ∪ K t), (9) where Fmerge is an LLM-based function that synthesizes a unified context preserving all distinct information from both units. This operation directly minimizes I(X1: t; Mt) by reducing |V t|, analogous to the AIB merge step that selects pairs with minimal JS-divergence. 

Link Operation. When scomp > τ l, a directed edge is established: 

ERELATED ← ERELATED ∪ { (nt, n i)}. (10) 

Remark 3.2 (Information-Theoretic Interpretation of Link) .

While Link does not directly reduce I(X1: t; Mt) like Merge, it preserves conditional dependencies that support I(Mt; Y ).Formally, Link is triggered when: 

I(nt; Y | ni) > 0 ∧ I(nt; ni) > 0, (11) indicating that nt provides additional task-relevant infor-mation beyond ni, and the two are logically related. By explicitly encoding this relationship in ERELATED , we pre-serve conditional structure necessary for multi-hop reason-ing without increasing representational redundancy. 

Append Operation. When neither threshold is met, nt is appended as an autonomous unit, preserving distributional diversity for novel content. 

Remark 3.3 (Extension Beyond Classical AIB) . The original AIB algorithm supports only merge operations on fixed co-occurrence matrices. M EM FLY extends this framework with Link and Append operations to handle streaming settings where information arrives incrementally and may exhibit complementary or novel content. This extension maintains the greedy optimization spirit while adapting to agentic memory requirements. 3.3.3. T OPIC EVOLUTION .Maintaining O(1) macro-navigability requires periodic re-structuring of the Topic layer. We formalize this as con-strained graph partitioning over the Keyword co-occurrence graph Gkw :

max  

> T

Q(T , Gkw )

s.t. δmin ≤ | Ci| ≤ δmax , ∀Ci ∈ T , (12) where Q denotes the modularity function and δmin , δ max 

are cardinality bounds. We employ the Leiden algorithm (Traag et al., 2019) for ef-ficiency. While modularity optimization differs from direct IB clustering, empirical studies demonstrate strong corre-lation between modularity-based and information-theoretic community structures (Fortunato, 2010). 5MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

3.4. Memory Retrieval 

3.4.1. T RI -P ATHWAY HYBRID RETRIEVAL .To exploit the optimized memory structure, M EM FLY em-ploys a tri-pathway hybrid retrieval strategy. Unlike conven-tional flat vector search, our approach decomposes queries into complementary semantic signals and executes parallel traversals over the memory graph. The raw query q is processed by an LLM-based semantic parser Fθ to disentangle retrieval intent: 

(htopic , Hkeys ) ← F θ (q), (13) where htopic ∈ Rd encodes the topical description, and 

Hkeys = {hk1 , . . . , hkm } contains embeddings for core en-tities in the query. The intent signals drive three synergistic pathways: macro-semantic localization, micro-symbolic anchoring, and topo-logical expansion. 

Pathway 1: Macro-Semantic Localization. This path-way addresses the navigation challenge in large-scale mem-ory. Given htopic , we identify the top-Ktopic relevant Topic centroids: 

T ∗ = TopK Ktopic 

�cos( htopic , μC ) | C ∈ T , (14) where μC ∈ Rd is the centroid embedding of Topic C.Notes are retrieved by hierarchy traversal: 

Rtopic = {n ∈ N | ∃ k ∈ K n, ∃C ∈ T ∗, k ∈ C}. (15) 

Pathway 2: Micro-Symbolic Anchoring. This pathway addresses the precision challenge for entity-centric queries. Query entities are matched against the keyword index: 

K∗ = [ 

> hk∈Hkeys

TopK Kkey 

�cos( hk, ek′ ) | k′ ∈ K , (16) where ek′ ∈ Rd is the embedding of Keyword k′. Notes are retrieved via keyword membership: 

Rkey = {n ∈ N | K n ∩ K ∗̸ = ∅} . (17) 

Pathway 3: Topological Expansion. This pathway ad-dresses connectivity for multi-hop reasoning by retrieving evidence that is logically related but vectorially distant. Starting from the anchor set: 

Eanc = Rtopic ∪ R key , (18) we expand along the ERELATED edges established during consolidation: 

Eexpand = {m ∈ N | ∃ n ∈ E anc , (n, m ) ∈ ERELATED }.

(19) 

Evidence Fusion. The final evidence pool combines all pathways via Reciprocal Rank Fusion (RRF) (Cormack et al., 2009). RRF aggregates the reciprocal ranks of can-didates across different retrieval pathways, prioritizing evi-dence that consistently appears at the top of multiple lists without requiring score normalization. The final pool is: 

Epool = Top- Kfinal 

�score RRF ∪ E expand 

, (20) where score RRF denotes the fusion score calculated by RRF and Kfinal denotes the predefined budget for the final pool. 3.4.2. I TERATIVE EVIDENCE REFINEMENT 

Complex reasoning tasks may require evidence not directly accessible from the initial query. We address this through an Iterative Evidence Refinement (IER) protocol that pro-gressively expands the evidence pool. At each iteration i, the system evaluates whether the current evidence pool E(i) sufficiently addresses the query. This evaluation is performed by an LLM that assesses informa-tion completeness. Formally, we define the sufficiency pred-icate: Suf (E(i), q ) = 

(

1, if LLM (E(i), q ) = true 

0, otherwise (21) If gaps are identified, a refined sub-query q(i+1) is synthe-sized to target missing aspects, and retrieval is re-executed via the tri-pathway mechanism. The evidence pool is up-dated: 

E(i+1) = E(i) ∪ n ∈ R (q(i+1) ) | n / ∈ E (i) , (22) where R(q) denotes the tri-pathway retrieval function. This process continues until Suf (E(i), q ) = true or the maxi-mum iteration count Imax is reached. 

## 4. Experiments 

4.1. Experimental Setup Dataset. We evaluate M EM FLY on the LoCoMo bench-mark (Maharana et al., 2024), a dataset specifically designed to assess the long-term information synthesis capabilities of LLM agents. LoCoMo contains long-horizon conversations with interleaved topics and evolving entity states, making it a robust testbed for dynamic memory structures. To provide a granular analysis of memory performance, we evaluate on five distinct reasoning categories: Multi-Hop, Temporal, Open Domain, Single Hop, and Adversarial. 

Evaluation Metrics. Following standard evaluation met-rics (Xu et al., 2025), we employ two primary metrics: F1 Score to measure the token-level overlap and precision of the 6MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

Table 1. Main results on LoCoMo benchmark using closed-source models (GPT series). We report F1 and BLEU-1 (%) scores across five categories. The best performance in each category is marked in bold , and the second best is underlined. 

Model Method Category Average Multi Hop Temporal Open Domain Single Hop Adversial F1 BLEU F1 BLEU F1 BLEU F1 BLEU F1 BLEU F1 BLEU 4o-mini 

LOCOMO 25.02 19.75 18.41 14.77 12.04 11.16 40.36 29.05 69.23 68.75 39.74 33.47 READ AGENT 9.15 6.48 12.60 8.87 5.31 5.12 9.67 7.66 9.81 9.02 9.89 7.87 MEMORY BANK 5.00 4.77 9.68 6.99 5.56 5.94 6.61 5.16 7.36 6.48 6.99 5.73 MEM GPT 26.65 17.72 25.52 19.44 9.15 7.44 41.04 34.34 43.29 42.73 35.45 30.16 A-MEM 27.02 20.09 45.85 36.67 12.14 12.00 44.65 37.06 50.03 49.47 41.97 36.16 MEM -0 34.72 25.13 45.93 35.51 22.64 15.58 43.65 37.42 30.15 27.44 38.70 32.07 MEM FLY 32.11 24.48 46.61 31.84 23.98 16.84 44.74 38.17 51.48 51.96 43.76 37.27 4o 

LOCOMO 28.00 18.47 9.09 5.78 16.47 14.80 61.56 54.19 52.61 51.13 44.12 38.70 

READ AGENT 14.61 9.95 4.16 3.19 8.84 8.37 12.46 10.29 6.81 6.13 9.98 8.07 MEMORY BANK 6.49 4.69 2.47 2.43 6.43 5.30 8.28 7.10 4.42 3.67 6.13 5.15 MEM GPT 30.36 22.83 17.29 13.18 12.24 11.87 60.16 53.35 34.96 34.25 41.02 36.23 A-MEM 32.86 23.76 39.41 31.23 17.10 15.84 48.43 42.97 36.35 35.53 40.53 35.36 MEM -0 35.13 27.56 52.38 44.15 17.73 15.92 39.12 35.43 25.44 24.19 36.59 32.25 MEM FLY 35.89 29.24 39.78 27.12 25.74 19.53 49.08 43.05 48.24 48.92 44.39 38.70 

Table 2. Main results on LoCoMo benchmark using open-source models (Qwen series). We report F1 and BLEU-1 (%) scores across five reasoning categories. The best performance in each category is marked in bold , and the second best is underlined. 

Model Method Category Average Multi Hop Temporal Open Domain Single Hop Adversial F1 BLEU F1 BLEU F1 BLEU F1 BLEU F1 BLEU F1 BLEU Qwen3-8B 

LOCOMO 25.09 15.73 32.82 27.14 14.47 13.35 20.18 18.39 46.77 40.81 28.62 24.22 READ AGENT 13.17 9.30 34.91 27.04 8.80 7.45 26.44 24.83 29.98 28.34 25.87 22.93 MEMORY BANK 21.25 14.53 30.20 21.11 11.33 10.53 32.75 26.33 30.95 30.13 29.27 23.90 MEM GPT 22.13 13.44 31.47 22.16 14.51 13.54 33.49 34.12 34.58 31.44 30.88 27.67 A-MEM 24.30 16.90 34.50 23.10 13.10 12.20 38.10 33.30 31.00 30.10 32.76 27.58 MEM -0 23.04 19.74 29.65 23.16 20.63 13.75 30.46 25.62 26.02 22.48 27.80 23.11 MEM FLY 28.24 22.76 38.39 33.64 15.43 13.81 42.09 36.57 43.79 43.14 38.62 34.51 Qwen3-14B 

LOCOMO 33.37 24.26 31.49 16.42 13.92 11.02 25.46 24.82 49.17 35.00 32.42 25.02 READ AGENT 13.16 9.61 18.12 12.33 12.16 9.25 32.83 28.35 5.96 4.2 20.63 16.75 MEMORY BANK 25.97 18.16 25.37 18.76 13.52 11.69 34.92 30.6 21.94 17.56 28.16 23.08 MEM GPT 24.12 15.41 25.48 19.04 13.44 12.64 34.74 32.41 27.11 24.32 28.99 25.06 A-MEM 21.36 14.98 23.06 18.04 12.62 11.49 35.43 30.92 26.71 25.78 28.37 24.48 MEM -0 20.98 16.27 31.5 21.73 12.7 13.22 24.7 19.14 21.01 19.84 23.86 19.02 MEM FLY 30.80 23.13 29.25 24.56 14.11 11.03 42.25 35.52 26.59 25.02 33.65 28.45 

answer spans, and BLEU-1 (Papineni et al., 2002) to eval-uate the lexical fidelity of the generated responses against ground truth. For ablation studies, we additionally report Recall, measuring the proportion of ground-truth evidence retrieved, and Hit Rate, indicating whether any relevant evidence appears in the candidates. 

Implementation Details. We implement M EM FLY using a triple-layer graph architecture backed by Neo4j, integrating both vector indices and explicit topological relationships. For retrieval, we set Ktopic = 3 for Topic-based navigation, 

Kkey = 10 for Keyword anchoring, Kfinal = 20 for the final retrieval pool size, and perform 1-hop traversal along 

ERELATED edges for topological expansion. The iterative refinement protocol uses Imax = 3 iterations. For memory construction, we set the merge threshold τm = 0 .7 and link threshold τl = 0 .5 based on validation performance. 

Backbone Models and Baselines. We evaluate M EM -FLY across four foundation models spanning closed-source (GPT (OpenAI, 2024)) and open-source (Qwen (Qwen, 2025)) families: GPT-4o-mini, GPT-4o, Qwen3-8B, and Qwen3-14B. The generation temperature is set to 0.7

for general reasoning and 0.5 for adversarial tasks. We compare M EM FLY against six representative methods: LOCOMO (Maharana et al., 2024), R EAD AGENT (Lee et al., 2024), MEMORY BANK (Zhong et al., 2023), MEM GPT (Packer et al., 2024), A-MEM (Xu et al., 2025), and M EM 0 (Chhikara et al., 2025). All baselines are im-plemented using their official system prompts and default configurations to ensure a fair comparison. 

4.2. Main Results Overall Performance. Tables 1 and 2 present perfor-mance comparisons on closed-source and open-source mod-els, respectively. M EM FLY achieves the highest average 7MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

Table 3. Ablation study on LoCoMo (Qwen3-8B). We evalu-ate memory construction and retrieval components. Average F1, BLEU-1, Recall, and Hit Rate (%) are reported. The best perfor-mance in each category is marked in bold , and the second best is underlined. 

Phase Method F1 BLEU Recall Hit Rate 

- MEM FLY 38.62 36.85 62.22 67.12 Constru-ction  w/o Update 27.97 27.10 42.11 48.20 w/o Denoise 36.07 34.68 57.42 62.55 w/o Link 33.57 32.35 53.19 56.18 w/o Merge 34.79 33.62 54.85 59.42 

> Retrieval

w/o Topic 36.79 34.66 53.30 58.91 w/o Keyword 32.69 33.94 51.28 54.26 w/o Neighbor 34.26 32.85 51.28 54.35 w/o IER 32.94 30.86 46.29 51.26 

F1 and BLEU-1 scores across all four backbone models. On closed-source models, it attains 43.76% and 44.39% F1 on GPT-4o-mini and GPT-4o respectively, outperforming the strongest baseline by 1.79 and 0.27 points. The advan-tage becomes more pronounced on open-source models: on Qwen3-8B, M EM FLY achieves 38.62% F1, surpassing the second-best A-MEM by 5.86 points. This larger margin on open-source models suggests that our structured memory organization effectively compensates for weaker in-context reasoning capabilities. The consistent improvements across heterogeneous architectures validate the generalization of our approach. 

Category-wise Analysis. Among the five reasoning cat-egories, M EM FLY demonstrates the largest gains on Open Domain queries, achieving 25.74% F1 on GPT-4o compared to 17.73% for M EM -0. This improvement can be attributed to Topic-based navigation that localizes relevant memory regions before fine-grained retrieval. For Single Hop tasks requiring precise entity matching, M EM FLY achieves top performance on both Qwen models (42.09% and 42.25% F1), indicating effective Keyword-based anchoring. 

4.3. Ablation Study Memory Construction Ablation. We examine the impact of IB-based memory consolidation by disabling core con-struction mechanisms. Removing the entire gated update (w/o Update ) causes the most severe degradation, with aver-age F1 dropping from 38.62% to 27.97% and Recall declin-ing from 62.22% to 42.11%. As shown in Figure 2(a), this variant exhibits the largest performance gap across all five categories, with Adversarial and Temporal showing the most pronounced decline. This confirms that without active con-solidation, noise accumulates and temporal dependencies become disrupted. Among individual operations, w/o Link 

shows larger impact than w/o Merge (33.57% vs 34.79% F1), and Figure 2(a) reveals that Link removal particularly affects Adversarial performance, indicating that associative Multi Hop Temporal Open Domain Single Hop Adversarial     

> 10
> 20
> 30
> 40
> F1 (%)
> (a) Construction Phase
> Full
> w/o Update
> w/o Denoise
> w/o Link
> w/o Merge
> Multi Hop Temporal Open Domain Single Hop Adversarial
> (b) Retrieval Phase
> Full
> w/o Topic
> w/o Keyword
> w/o Neighbor
> w/o IER

Figure 2. Category-wise F1 scores (%) for ablation variants on LoCoMo (Qwen3-8B). (a) Ablations on memory construction components. (b) Ablations on retrieval pathways and iterative refinement. 

edges are critical for filtering distractors. The w/o Denoise 

variant achieves the second-best performance (36.07% F1), maintaining relatively stable results across all categories as shown in the figure, suggesting that semantic preprocessing provides consistent but auxiliary improvements. 

Memory Retrieval Ablation. We systematically disable each retrieval pathway to assess individual contributions. The w/o Topic achieves the second-best performance among retrieval ablations (36.79% F1), and Figure 2(b) shows rel-atively uniform degradation across categories, indicating that macro-semantic navigation provides general retrieval guidance. In contrast, w/o Keyword (32.69% F1) and w/o IER (32.94% F1) exhibit more category-specific impacts. As illustrated in Figure 2(b), Keyword removal causes the most pronounced decline on Single-Hop queries, validat-ing that symbolic anchoring is essential for precise entity matching. The w/o IER variant shows the largest degra-dation on Adversarial and Open Domain categories in the figure, demonstrating that iterative refinement is critical for queries requiring progressive evidence accumulation. The 

w/o Neighbor variant (34.26% F1) primarily impacts Adver-sarial, confirming that topological expansion via ERELATED 

edges helps distinguish relevant evidence from distractors. 

## 5. Conclusion 

We presented M EM FLY , a framework that formulates agen-tic long-term memory as an Information Bottleneck prob-lem. Our approach employs an LLM-based gradient-free optimizer to consolidate redundant information while pre-serving task-relevant evidence through a stratified Note-Keyword-Topic hierarchy. The tri-pathway retrieval mech-anism with iterative refinement effectively exploits this structure for complex reasoning. Experiments on LoCoMo demonstrate consistent improvements over state-of-the-art baselines across diverse backbone models. 

Limitations. The current implementation prioritizes mem-ory quality over construction speed, introducing moderate computational overhead. Extending evaluation to multi-modal and domain-specific scenarios remains an avenue for future investigation. 8MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

## Impact Statement 

This paper presents work whose goal is to advance the field of Machine Learning. There are many potential societal consequences of our work, none which we feel must be specifically highlighted here. 

## References 

Asai, A., Wu, Z., Wang, Y., Sil, A., and Hajishirzi, H. Self-rag: Learning to retrieve, generate, and critique through self-reflection, 2023. Chen, H., Pasunuru, R., Weston, J., and Celikyilmaz, A. Walking down the memory maze: Beyond con-text limit through interactive reading. arXiv preprint arXiv:2310.05029 , 2023. Chhikara, P., Khant, D., Aryan, S., Singh, T., and Yadav, D. Mem0: Building production-ready ai agents with scalable long-term memory, 2025. Cormack, G. V., Clarke, C. L. A., and Buettcher, S. Recip-rocal rank fusion outperforms condorcet and individual rank learning methods. In Proceedings of the 32nd In-ternational ACM SIGIR Conference on Research and Development in Information Retrieval . Association for Computing Machinery, 2009. Edge, D., Trinh, H., Cheng, N., Bradley, J., Chao, A., Mody, A., Truitt, S., Metropolitansky, D., Ness, R. O., and Lar-son, J. From local to global: A graph rag approach to query-focused summarization, 2025. Fang, J., Deng, X., Xu, H., Jiang, Z., Tang, Y., Xu, Z., Deng, S., Yao, Y., Wang, M., Qiao, S., Chen, H., and Zhang, N. Lightmem: Lightweight and efficient memory-augmented generation, 2025. URL https://arxiv. org/abs/2510.18866 .Ferrag, M. A., Tihanyi, N., and Debbah, M. From llm reasoning to autonomous ai agents: A comprehensive review, 2025. URL https://arxiv.org/abs/ 2504.19678 .Fortunato, S. Community detection in graphs. Physics Reports , 486(3–5):75–174, February 2010. ISSN 0370-1573. doi: 10.1016/j.physrep.2009.11.002. URL 

http://dx.doi.org/10.1016/j.physrep. 2009.11.002 .Gao, Y., Xiong, Y., Gao, X., Jia, K., Pan, J., Bi, Y., Dai, Y., Sun, J., Wang, M., and Wang, H. Retrieval-augmented generation for large language models: A survey, 2024. URL https://arxiv.org/abs/2312.10997 .Hans, A., Wen, Y., Jain, N., Kirchenbauer, J., Kazemi, H., Singhania, P., Singh, S., Somepalli, G., Geiping, J., Bhatele, A., and Goldstein, T. Be like a goldfish, don’t memorize! mitigating memorization in genera-tive llms, 2024. URL https://arxiv.org/abs/ 2406.10209 .Hu, M., Chen, T., Chen, Q., Mu, Y., Shao, W., and Luo, P. Hiagent: Hierarchical working memory management for solving long-horizon agent tasks with large language model, 2024. Lee, K.-H., Chen, X., Furuta, H., Canny, J., and Fischer, I. A human-inspired reading agent with gist memory of very long contexts, 2024. Lewis, P., Perez, E., Piktus, A., Petroni, F., Karpukhin, V., Goyal, N., K ¨uttler, H., Lewis, M., Yih, W.-t., Rockt ¨aschel, T., Riedel, S., and Kiela, D. Retrieval-augmented genera-tion for knowledge-intensive nlp tasks, 2021. Maharana, A., Lee, D.-H., Tulyakov, S., Bansal, M., Barbi-eri, F., and Fang, Y. Evaluating very long-term conversa-tional memory of llm agents, 2024. OpenAI. Gpt-4 technical report, 2024. URL https:// arxiv.org/abs/2303.08774 .Packer, C., Wooders, S., Lin, K., Fang, V., Patil, S. G., Stoica, I., and Gonzalez, J. E. Memgpt: Towards llms as operating systems, 2024. Papineni, K., Roukos, S., Ward, T., and Zhu, W.-J. Bleu: a method for automatic evaluation of machine transla-tion. In Annual Meeting of the Association for Com-putational Linguistics , 2002. URL https://api. semanticscholar.org/CorpusID:11080756 .Qwen. Qwen3 technical report, 2025. URL https:// arxiv.org/abs/2505.09388 .Ram, O., Levine, Y., Dalmedigos, I., Muhlgay, D., Shashua, A., Leyton-Brown, K., and Shoham, Y. In-context retrieval-augmented language models, 2023. URL 

https://arxiv.org/abs/2302.00083 .Shinn, N., Cassano, F., Berman, E., Gopinath, A., Narasimhan, K., and Yao, S. Reflexion: Language agents with verbal reinforcement learning, 2023. URL 

https://arxiv.org/abs/2303.11366 .Slonim, N. and Tishby, N. Agglomerative information bot-tleneck. In Advances in Neural Information Processing Systems , volume 12. MIT Press, 1999. Slonim, N. and Tishby, N. Document clustering using word clusters via the information bottleneck method. In Pro-ceedings of the 23rd Annual International ACM SIGIR Conference on Research and Development in Information Retrieval , SIGIR ’00, pp. 208–215, New York, NY, USA, 9MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

2000. Association for Computing Machinery. ISBN 978-1-58113-226-7. doi: 10.1145/345508.345578. Sumers, T. R., Yao, S., Narasimhan, K., and Griffiths, T. L. Cognitive architectures for language agents, 2024. URL 

https://arxiv.org/abs/2309.02427 .Tishby, N. and Zaslavsky, N. Deep learning and the in-formation bottleneck principle, 2015. URL https: //arxiv.org/abs/1503.02406 .Traag, V. A., Waltman, L., and van Eck, N. J. From louvain to leiden: guaranteeing well-connected communities. Sci-entific Reports , 9(1), March 2019. ISSN 2045-2322. doi: 10.1038/s41598-019-41695-z. URL http://dx.doi. org/10.1038/s41598-019-41695-z .Wang, L., Ma, C., Feng, X., Zhang, Z., Yang, H., Zhang, J., Chen, Z., Tang, J., Chen, X., Lin, Y., Zhao, W. X., Wei, Z., and Wen, J. A survey on large language model based autonomous agents. Frontiers of Computer Science ,18(6), March 2024. ISSN 2095-2236. doi: 10.1007/ s11704-024-40231-1. URL http://dx.doi.org/ 10.1007/s11704-024-40231-1 .Wang, P., Tian, M., Li, J., Liang, Y., Wang, Y., Chen, Q., Wang, T., Lu, Z., Ma, J., Jiang, Y. E., and Zhou, W. O-mem: Omni memory system for personalized, long horizon, self-evolving agents, 2025. Wu, X., Yang, C., Lin, X., Xu, C., Jiang, X., Sun, Y., Xiong, H., Li, J., and Guo, J. Think-on-graph 3.0: Efficient and adaptive llm reasoning on heterogeneous graphs via multi-agent dual-evolving context retrieval, 2025. Xi, Z., Chen, W., Guo, X., He, W., Ding, Y., Hong, B., Zhang, M., Wang, J., Jin, S., Zhou, E., Zheng, R., Fan, X., Wang, X., Xiong, L., Liu, Q., Zhou, Y., Wang, W., Jiang, C., Zou, Y., Liu, X., Yin, Z., Dou, S., Weng, R., Cheng, W., Zhang, Q., Qin, W., Zheng, Y., Qiu, X., Huan, X., and Gui, T. The rise and potential of large language model based agents: A survey. ArXiv , abs/2309.07864, 2023. URL https://api.semanticscholar. org/CorpusID:261817592 .Xu, W., Mei, K., Gao, H., Tan, J., Liang, Z., and Zhang, Y. A-mem: Agentic memory for llm agents, 2025. Yan, S.-Q., Gu, J.-C., Zhu, Y., and Ling, Z.-H. Corrective retrieval augmented generation, 2024. Yang, C., Wang, X., Lu, Y., Liu, H., Le, Q. V., Zhou, D., and Chen, X. Large language models as optimizers, 2024. URL https://arxiv.org/abs/2309.03409 .Zhai, Y., Tao, S., Chen, C., Zou, A., Chen, Z., Fu, Q., Mai, S., Yu, L., Deng, J., Cao, Z., Liu, Z., Ding, B., and Zhou, J. Agentevolver: Towards efficient self-evolving agent sys-tem, 2025. URL https://arxiv.org/abs/2511. 10395 .Zhang, G., Fu, M., Wan, G., Yu, M., Wang, K., and Yan, S. G-memory: Tracing hierarchical memory for multi-agent systems, 2025. URL https://arxiv.org/ abs/2506.07398 .Zhong, W., Guo, L., Gao, Q., Ye, H., and Wang, Y. Memo-rybank: Enhancing large language models with long-term memory, 2023. 10 MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

## A. Prompting Templates 

This appendix provides the complete prompting templates used in M EM FLY . All prompts are designed to elicit structured JSON outputs for reliable parsing. 

A.1. Memory Construction Prompts 

A.1.1. S EMANTIC INGESTION PROMPT 

During the ingestion phase (Sec. 3.3.1), raw conversational input xt is transformed into a structured Note nt = ( rt, c t, h t, K t).The following prompt instructs the LLM to extract keywords Kt and generate the denoised context ct:Semantic Ingestion Prompt 

You are an expert Knowledge Graph Extractor. Your task is to analyze the [TARGET TURN] to extract structured metadata. 

Input Text: {content }

Guidelines: 1. Keywords (Entities): 

• GOAL : Extract 3-5 specific Noun Phrases explicitly present in the text. • FOCUS PRIORITIES :1. Proper Nouns: People (e.g., “Melanie”), Locations, Organizations. 2. Concrete Objects: Physical items (e.g., “painting”, “plate”, “contract”). 3. Specific Topics: “LGBTQ support group”, “deadline”. • CRITICAL STOP LIST : Ignore conversational meta-roles, abstract terms, and speaker names acting purely as subjects. • ZERO-SHOT RULE : If the text is purely phatic (e.g., “Wow”, “That’s cool”), return an empty list. 

2. Context (Factual Restatement): 

• GOAL : Rewrite the text into a self-contained factual statement. • CONSTRAINT 1 (Strict Fidelity) : Only use information present in the Input Text. • CONSTRAINT 2 (Safe Resolution) : Resolve “I/my/we” using the Speaker’s name if it appears in the text. For external pronouns where the antecedent is missing, keep the pronoun or use a generic term. Do not guess. • CONSTRAINT 3 (No Meta-Language) : Start directly with the subject. Avoid “The speaker says...”. 

Output Format (JSON): 

{"keywords": ["entity1", "entity2"], "context": "Melanie thinks the item is cool."} 

The extracted keywords are matched against the existing Keyword index K to establish symbolic anchors, while the context is encoded via the embedding model to obtain ht = Embed (ct). This dual extraction enables both symbolic and semantic access pathways during retrieval. A.1.2. G ATED STRUCTURAL UPDATE PROMPT 

During the gated structural update phase (Sec. 3.3.2), the LLM policy evaluates the relationship between a new Note nt and each candidate Note ni ∈ N cand . The following prompt generates the redundancy score sred and complementarity score 

scomp , and determines the appropriate structural operation (Merge, Link, or Append). Gated Structural Update Prompt 

Role: You are a Knowledge Graph Updater. Your job is to evaluate the relationship between a NEW NODE and existing CANDIDATE NODES. 

[NEW NODE] 

Content: “ {content }”Context: “ {context }”Keywords: {keywords }

[CANDIDATE NODES] 

{candidates str }

Instructions: 

Analyze each candidate and generate a JSON response following these rules: 

1. Analyze Relationship: 

• Determine relation type : ‘SUPPORTS’, ‘CONFLICTS’, or ‘RELATED TO’. • Assign connection strength (0.0 – 1.0), indicating the degree of semantic overlap or logical connection. 

2. Determine Operation (Based on Strength): 

• CASE A: Strength ≥ 0.8 (High Redundancy) → MERGE – Action: Integrate details from the New Node into the candidate’s context. 

– Template: “[Original Context]. Specifically, [New Node Info]...” • CASE B: Strength ∈ [0.5, 0.8) (Complementary) → LINK – Action: Establish associative edge; keep contexts separate. 

– Template: “[Original Context]. (Related: [New Node Keyword])” • CASE C: Strength < 0.5 (Distinct) → APPEND – Action: Add New Node as autonomous unit; no modification. 

11 MemFly: On-the-Fly Memory Optimization via Information Bottleneck      

> •CASE D: relation type is ‘CONFLICTS’ →LINK with Contrast –Action: Note the conflicting information explicitly.
> –Template: “[Original Context]. However, [New Node] indicates that...”
> 3. Output : Return strictly valid JSON matching the schema.

Mapping to Paper Notation. The connection strength score directly corresponds to our redundancy score 

sred (nt, n i) when the relation type indicates semantic overlap, and to the complementarity score scomp (nt, n i) when the nodes contain distinct but logically related information. The threshold τm = 0 .7 for Merge and τl = 0 .5 for Link (Sec. 4) are applied to these scores to determine the final structural operation according to Eq. (6). When the Merge operation is triggered, the LLM generates a unified context c′ 

> i

= Fmerge (ci, c t) following the template specified in Case A, preserving all distinct information from both units while eliminating redundancy. 

A.2. Memory Retrieval Prompts 

A.2.1. Q UERY INTENT ANALYSIS PROMPT 

During the retrieval phase (Sec. 3.4.1), the raw query q is processed by a semantic parser Fθ to extract retrieval intent signals. The following prompt disentangles the query into a topical description htopic and entity keywords Hkeys for driving the tri-pathway retrieval mechanism. Query Intent Analysis Prompt                      

> Analyze the user query to identify its Target Taxonomy Category , extract key entities, and detect time-related intent.
> Task 1: topic desc (Target Category)
> •Predict the Taxonomy Category or Subject Heading this query falls under. •Style : Strict Noun Phrase (like a book chapter title or library category). •Constraint : Keep it under 8 words. •Do NOT describe the user’s intent (e.g., avoid “how to...”, “techniques for...”). Instead, name the topic itself.
> Task 2: Keywords
> •Extract 3-5 core entities, technical terms, or specific concepts. •CRITICAL : Convert terms to their canonical singular form (e.g., “transformers” →“Transformer”). •Exclude generic verbs or stop words.
> Query: {query }
> Output Format (JSON):
> {"topic_desc": "Concise Noun Phrase", "keywords": ["keyword1", "keyword2", "keyword3"]}

Mapping to Paper Notation. The topic desc field is encoded via the embedding model to obtain htopic ∈ Rd,which drives Pathway 1 (Macro-Semantic Localization) through Topic matching (Eq. (14) ). The keywords are similarly embedded to form Hkeys = {hk1 , . . . , h km }, enabling Pathway 2 (Micro-Symbolic Anchoring) via Keyword matching (Eq. (16)). A.2.2. I TERATIVE EVIDENCE REFINEMENT PROMPTS 

The Iterative Evidence Refinement protocol (Sec. 3.4.2) employs two complementary prompts: a Sufficiency Evaluator that assesses whether the current evidence pool adequately addresses the query, and a Sub-query Generator that synthesizes targeted follow-up queries when gaps are identified. 

Sufficiency Evaluation Prompt. At each iteration i, the following prompt evaluates whether the current evidence pool E(i)

satisfies the sufficiency predicate Suf (E(i), q ) defined in Eq. (21). Sufficiency Evaluation Prompt  

> You are a reflector agent that evaluates whether the current context and answer is sufficient to answer a question.
> Question: {question }
> Current Context and Current Answer:
> {context }
> Evaluate whether the provided context and answer contains enough information to answer the question comprehensively.

12 MemFly: On-the-Fly Memory Optimization via Information Bottleneck           

> If the context and answer is insufficient, identify what specific information is missing.
> Output Format (JSON):
> {"sufficient": true or false, "missing_info": "description of missing information", "confidence": 0.0 to 1.0}

Sub-query Generation Prompt. When the sufficiency evaluation returns sufficient: false , the following prompt generates a refined sub-query q(i+1) targeting the identified information gaps. Sub-query Generation Prompt        

> You are a Query Evolution Agent. Your goal is to decompose a complex user question into a specific, actionable sub-query to retrieve missing information from a Knowledge Graph.
> Original Question: “{query str }”
> Current Known Information (Context):
> {context str }
> History of Reasoning Steps (Q&A):
> {prev reasoning }
> CRITICAL : The Reflector Agent has identified the following MISSING INFORMATION needed to answer the main question:
> {missing info }
> Task: Based on the “Missing Information” and “History”, formulate the NEXT single sub-question to retrieve this missing info. •The sub-question must be specific. •It should act as a search query for the next hop. •If we have enough information to answer the main question, return ‘None’.
> Sub-question:

IER Protocol Flow. The two prompts work in tandem: the Sufficiency Evaluator determines whether to terminate (when 

sufficient: true or iteration count reaches Imax ), while the Sub-query Generator drives evidence expansion by producing targeted queries that are re-executed through the tri-pathway retrieval mechanism (Eq. (22) ). The confidence 

score from the Sufficiency Evaluator can optionally be used for early termination when confidence exceeds a predefined threshold. 

## B. Dataset Statistics 

Table 4 presents the sample distribution across the five reasoning categories. The categories are designed to test distinct memory capabilities: Multi-Hop requires synthesizing evidence across multiple memory units; Temporal tests reasoning about time-dependent information and event ordering; Open Domain evaluates retrieval of general knowledge from conversation history; Single Hop assesses precise entity matching and direct fact retrieval; and Adversarial challenges the system with distractors and misleading information. As shown in Table 4, the category distribution is imbalanced, with Single Hop comprising the largest proportion (42.3%) and Open Domain the smallest (4.8%). To account for this imbalance, the average scores reported in Table 1 and Table 2 are computed as weighted averages based on category sample sizes, ensuring that performance on larger categories contributes proportionally to the overall evaluation. 

Table 4. LoCoMo benchmark category distribution. 

Category Samples Proportion Multi-Hop 282 14.2% Temporal 321 16.2% Open Domain 96 4.8% Single Hop 841 42.3% Adversarial 446 22.5% Total 1,986 100% 13 MemFly: On-the-Fly Memory Optimization via Information Bottleneck 

## C. Hyperparameter Settings 

Table 5 summarizes all hyperparameters used in M EM FLY . These hyperparameters are organized by the two main phases of our framework: memory construction and memory retrieval. For memory construction, the merge threshold τm = 0 .7 and link threshold τl = 0 .5 control the gated structural update decisions (Eq. (6) ). A higher merge threshold ensures that only highly redundant information is consolidated, preserving fine-grained distinctions between memory units. The link threshold is set lower to capture complementary relationships that support multi-hop reasoning. For memory retrieval, we set Ktopic = 3 to balance navigation precision with coverage, allowing the system to explore multiple relevant Topic clusters. The keyword retrieval parameter Kkey = 10 provides sufficient anchor points for entity-centric queries. The final pool size Kf inal = 20 bounds the evidence passed to the generation stage, balancing context richness against computational cost. The maximum IER iterations Imax = 3 prevents excessive retrieval loops while allowing sufficient evidence expansion for complex queries. All hyperparameters were tuned on a held-out validation set. We found the framework to be relatively robust to moderate variations in these values, with performance degrading gracefully when parameters deviate within ±20% of the reported settings.  

> Table 5. Hyperparameter settings.

Phase Parameter Value Construction Merge threshold τm 0.7 Link threshold τl 0.5 Retrieval Topic retrieval Ktopic 3Keyword retrieval Kkey 10 Final pool size Kf inal 20 Max IER iterations Imax 3Generation Temperature (general) 0.7 Temperature (adversarial) 0.5 14
