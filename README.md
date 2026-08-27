# Evidence for `hayro-interpret: Warn when an annotation appearance cannot be resolved`

This branch only carries attachments for the pull request against `LaurenzV/hayro`; it contains no
code and is not meant to be merged. `hayro-before-0.7.1.png` and `hayro-after-fork.png` render
`hayro-tests/pdfs/custom/annotation_checkbox.pdf` under released hayro 0.7.1 and under current
`main` respectively, showing that the rendering half of this problem was already fixed upstream in
commit 6af63be9 and is simply not released yet. `unresolved_state.pdf` is a 702-byte hand-written
reproducer for the half that is still silent: its single widget annotation has an `AP` entry whose
`N` is a state dictionary containing only `Yes`, while `AS` names `Nope` and there is no `Off`
entry to fall back on, so no appearance stream can be selected and the widget is skipped. On `main`
that skip produces no warning at all; with the commit in the pull request it emits
`InterpreterWarning::UnresolvedAnnotationAppearance`.
