//! Pure deterministic evaluator for the proposed language-neutral v0.1 token
//! contract. It takes immutable snapshots and never writes document state.

use std::collections::{BTreeMap, HashSet};

use crate::contract::{
    validate_number, ContextMap, Diagnostic, DiagnosticCode, DimensionUnit, Expression,
    ExpressionOperator, ModeStrategy, PropertyValue, ResolutionContext, ResolutionSource,
    ResolveRequest, ResolveResult, ResolveStatus, Severity, TokenDefinition, TokenReference,
    TokenSnapshot, TokenType, TypedValue, CONTRACT_VERSION, MAX_ALIAS_HOPS, MAX_CONTEXT_DIMENSIONS,
    MAX_EXPRESSION_DEPTH, MAX_EXPRESSION_NODES,
};

/// Resolve one versioned property request against an immutable token snapshot.
pub fn resolve(snapshot: &TokenSnapshot, request: &ResolveRequest) -> ResolveResult {
    let mut evaluator = Evaluator::new(snapshot, request);
    evaluator.resolve()
}

struct Evaluator<'a> {
    snapshot: &'a TokenSnapshot,
    request: &'a ResolveRequest,
    trace: Vec<TokenReference>,
    nodes: usize,
    diagnostics: Vec<Diagnostic>,
    effective_context: ContextMap,
}

impl<'a> Evaluator<'a> {
    fn new(snapshot: &'a TokenSnapshot, request: &'a ResolveRequest) -> Self {
        Self {
            snapshot,
            request,
            trace: Vec::new(),
            nodes: 0,
            diagnostics: Vec::new(),
            effective_context: ContextMap::new(),
        }
    }

    fn resolve(&mut self) -> ResolveResult {
        let validation = validate_snapshot(self.snapshot);
        if !validation.is_empty() {
            return self.error_result(validation);
        }
        if self.request.contract_version != CONTRACT_VERSION {
            return self.error_result(vec![self.diagnostic(DiagnosticCode::UnsupportedFeature)]);
        }
        if self.request.snapshot_revision != self.snapshot.revision {
            return self.error_result(vec![self.diagnostic(DiagnosticCode::StaleRevision)]);
        }

        let context = match compose_context(&self.request.context) {
            Ok(context) => context,
            Err(code) => return self.error_result(vec![self.diagnostic(code)]),
        };
        self.effective_context = context;

        let result = match &self.request.property {
            PropertyValue::Literal { value, .. } => {
                if value.token_type() != self.request.property_type {
                    Err(DiagnosticCode::TypeMismatch)
                } else {
                    value
                        .validate()
                        .map(|()| value.clone())
                        .map_err(|code| code)
                }
                .map(|value| (value, ResolutionSource::Direct))
            }
            PropertyValue::Binding {
                target,
                mode_strategy,
                fallback,
                local_override,
                ..
            } => {
                if let Some(value) = local_override {
                    if value.token_type() != self.request.property_type {
                        Err(DiagnosticCode::TypeMismatch)
                    } else {
                        value
                            .validate()
                            .map(|()| value.clone())
                            .map_err(|code| code)
                    }
                    .map(|value| (value, ResolutionSource::LocalOverride))
                } else {
                    let effective = match self.binding_context(target, mode_strategy) {
                        Ok(context) => context,
                        Err(code) => return self.binding_error_or_fallback(code, fallback),
                    };
                    self.effective_context = effective.clone();
                    let mut stack = Vec::new();
                    self.resolve_token(
                        target,
                        &self.request.property_type,
                        &effective,
                        &mut stack,
                        0,
                    )
                    .map(|value| (value, ResolutionSource::Token))
                }
            }
        };

        match result {
            Ok((value, source)) => self.success_result(value, source),
            Err(code) => match &self.request.property {
                PropertyValue::Binding { fallback, .. } => {
                    self.binding_error_or_fallback(code, fallback)
                }
                PropertyValue::Literal { .. } => self.error_result(vec![self.diagnostic(code)]),
            },
        }
    }

    fn binding_error_or_fallback(
        &mut self,
        code: DiagnosticCode,
        fallback: &Option<TypedValue>,
    ) -> ResolveResult {
        if let Some(value) = fallback {
            if value.token_type() == self.request.property_type && value.validate().is_ok() {
                self.diagnostics
                    .push(self.diagnostic_with_severity(code, Severity::Warning));
                return self.success_result(value.clone(), ResolutionSource::Fallback);
            }
            self.diagnostics
                .push(self.diagnostic(DiagnosticCode::TypeMismatch));
        }
        self.diagnostics.push(self.diagnostic(code));
        let diagnostics = std::mem::take(&mut self.diagnostics);
        self.error_result(diagnostics)
    }

    fn binding_context(
        &self,
        target: &TokenReference,
        strategy: &ModeStrategy,
    ) -> Result<ContextMap, DiagnosticCode> {
        let mut context = self.effective_context.clone();
        let definitions = self.definitions(target);
        let collection_id = definitions
            .first()
            .and_then(|definition| definition.collection_id);
        let Some(collection_id) = collection_id else {
            if matches!(strategy, ModeStrategy::Explicit { .. }) {
                return Err(DiagnosticCode::MissingMode);
            }
            return Ok(context);
        };
        let collection = self
            .snapshot
            .collections
            .iter()
            .find(|collection| collection.id == collection_id)
            .ok_or(DiagnosticCode::MissingMode)?;
        match strategy {
            ModeStrategy::Explicit { mode_id } => {
                let mode = collection
                    .modes
                    .iter()
                    .find(|mode| mode.id == *mode_id)
                    .ok_or(DiagnosticCode::MissingMode)?;
                context.insert(collection.mode_dimension.clone(), mode.option.clone());
            }
            ModeStrategy::Inherit => {
                if let Some(option) = context.get(&collection.mode_dimension) {
                    if !collection.modes.iter().any(|mode| mode.option == *option) {
                        return Err(DiagnosticCode::MissingMode);
                    }
                } else {
                    let mode = collection
                        .modes
                        .iter()
                        .find(|mode| mode.id == collection.default_mode_id)
                        .ok_or(DiagnosticCode::MissingMode)?;
                    context.insert(collection.mode_dimension.clone(), mode.option.clone());
                }
            }
        }
        if context.len() > MAX_CONTEXT_DIMENSIONS {
            return Err(DiagnosticCode::ContextBudget);
        }
        Ok(context)
    }

    fn resolve_token(
        &mut self,
        reference: &TokenReference,
        expected_type: &TokenType,
        context: &ContextMap,
        stack: &mut Vec<TokenReference>,
        alias_hops: usize,
    ) -> Result<TypedValue, DiagnosticCode> {
        if alias_hops >= MAX_ALIAS_HOPS {
            return Err(DiagnosticCode::AliasBudget);
        }
        self.trace.push(reference.clone());
        if stack.contains(reference) {
            return Err(DiagnosticCode::AliasCycle);
        }
        let definitions = self.definitions(reference);
        if definitions.is_empty() {
            return Err(DiagnosticCode::MissingToken);
        }
        let definition = self.choose_definition(&definitions, context)?.clone();
        if &definition.token_type != expected_type {
            return Err(DiagnosticCode::TypeMismatch);
        }
        let mut token_context = context.clone();
        if let Some(collection_id) = definition.collection_id {
            let collection = self
                .snapshot
                .collections
                .iter()
                .find(|collection| collection.id == collection_id)
                .ok_or(DiagnosticCode::MissingMode)?;
            if let Some(option) = token_context.get(&collection.mode_dimension) {
                if !collection.modes.iter().any(|mode| mode.option == *option) {
                    return Err(DiagnosticCode::MissingMode);
                }
            } else {
                let default_mode = collection
                    .modes
                    .iter()
                    .find(|mode| mode.id == collection.default_mode_id)
                    .ok_or(DiagnosticCode::MissingMode)?;
                token_context.insert(
                    collection.mode_dimension.clone(),
                    default_mode.option.clone(),
                );
            }
        }
        let selected = select_value(&definition, &token_context)?.clone();
        stack.push(reference.clone());
        let value = self.evaluate_expression(
            &selected.expression,
            &definition.token_type,
            &token_context,
            stack,
            alias_hops,
            1,
        );
        stack.pop();
        value
    }

    fn definitions<'b>(&'b self, reference: &TokenReference) -> Vec<&'b TokenDefinition> {
        self.snapshot
            .tokens
            .iter()
            .filter(|token| {
                token.id == reference.token_id && token.namespace == reference.namespace
            })
            .collect()
    }

    fn choose_definition<'b>(
        &self,
        definitions: &[&'b TokenDefinition],
        context: &ContextMap,
    ) -> Result<&'b TokenDefinition, DiagnosticCode> {
        for set_id in self.snapshot.active_set_ids.iter().rev() {
            let active_set = self
                .snapshot
                .sets
                .iter()
                .find(|set| set.id == *set_id)
                .ok_or(DiagnosticCode::InvalidSnapshot)?;
            if context_matches(&active_set.when, context) {
                if let Some(token) = definitions
                    .iter()
                    .find(|token| token.set_id == Some(*set_id))
                {
                    return Ok(token);
                }
            }
        }
        definitions
            .iter()
            .find(|token| token.set_id.is_none())
            .copied()
            .ok_or(DiagnosticCode::MissingToken)
    }

    fn evaluate_expression(
        &mut self,
        expression: &Expression,
        expected_type: &TokenType,
        context: &ContextMap,
        stack: &mut Vec<TokenReference>,
        alias_hops: usize,
        depth: usize,
    ) -> Result<TypedValue, DiagnosticCode> {
        self.nodes += 1;
        if self.nodes > MAX_EXPRESSION_NODES || depth > MAX_EXPRESSION_DEPTH {
            return Err(DiagnosticCode::ExpressionBudget);
        }
        let result = match expression {
            Expression::Literal { value } => {
                value.validate()?;
                if &value.token_type() != expected_type {
                    return Err(DiagnosticCode::TypeMismatch);
                }
                value.clone()
            }
            Expression::Reference { target } => {
                if alias_hops >= MAX_ALIAS_HOPS {
                    return Err(DiagnosticCode::AliasBudget);
                }
                let definitions = self.definitions(target);
                if definitions.is_empty() {
                    self.trace.push(target.clone());
                    return Err(DiagnosticCode::MissingToken);
                }
                let target_type = definitions[0].token_type.clone();
                if &target_type != expected_type {
                    return Err(DiagnosticCode::TypeMismatch);
                }
                self.resolve_token(target, &target_type, context, stack, alias_hops + 1)?
            }
            Expression::Operation {
                operator,
                arguments,
            } => {
                let required = match operator {
                    ExpressionOperator::Subtract | ExpressionOperator::Divide => Some(2),
                    ExpressionOperator::Clamp => Some(3),
                    _ => None,
                };
                if arguments.len() < 2 || required.is_some_and(|count| arguments.len() != count) {
                    return Err(DiagnosticCode::ExpressionInvalid);
                }
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.evaluate_untyped_expression(
                        argument,
                        context,
                        stack,
                        alias_hops,
                        depth + 1,
                    )?);
                }
                evaluate_operation(*operator, &values)?
            }
        };
        result.validate()?;
        if &result.token_type() != expected_type {
            return Err(DiagnosticCode::TypeMismatch);
        }
        Ok(result)
    }

    fn evaluate_untyped_expression(
        &mut self,
        expression: &Expression,
        context: &ContextMap,
        stack: &mut Vec<TokenReference>,
        alias_hops: usize,
        depth: usize,
    ) -> Result<TypedValue, DiagnosticCode> {
        match expression {
            Expression::Literal { value } => {
                self.nodes += 1;
                if self.nodes > MAX_EXPRESSION_NODES || depth > MAX_EXPRESSION_DEPTH {
                    return Err(DiagnosticCode::ExpressionBudget);
                }
                value.validate()?;
                Ok(value.clone())
            }
            Expression::Reference { target } => {
                self.nodes += 1;
                if self.nodes > MAX_EXPRESSION_NODES || depth > MAX_EXPRESSION_DEPTH {
                    return Err(DiagnosticCode::ExpressionBudget);
                }
                if alias_hops >= MAX_ALIAS_HOPS {
                    return Err(DiagnosticCode::AliasBudget);
                }
                let definitions = self.definitions(target);
                let Some(definition) = definitions.first() else {
                    self.trace.push(target.clone());
                    return Err(DiagnosticCode::MissingToken);
                };
                let target_type = definition.token_type.clone();
                self.resolve_token(target, &target_type, context, stack, alias_hops + 1)
            }
            Expression::Operation {
                operator,
                arguments,
            } => {
                self.nodes += 1;
                if self.nodes > MAX_EXPRESSION_NODES || depth > MAX_EXPRESSION_DEPTH {
                    return Err(DiagnosticCode::ExpressionBudget);
                }
                let required = match operator {
                    ExpressionOperator::Subtract | ExpressionOperator::Divide => Some(2),
                    ExpressionOperator::Clamp => Some(3),
                    _ => None,
                };
                if arguments.len() < 2 || required.is_some_and(|count| arguments.len() != count) {
                    return Err(DiagnosticCode::ExpressionInvalid);
                }
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.evaluate_untyped_expression(
                        argument,
                        context,
                        stack,
                        alias_hops,
                        depth + 1,
                    )?);
                }
                evaluate_operation(*operator, &values)
            }
        }
    }

    fn success_result(&mut self, value: TypedValue, source: ResolutionSource) -> ResolveResult {
        ResolveResult {
            contract_version: CONTRACT_VERSION.to_owned(),
            snapshot_revision: self.snapshot.revision.clone(),
            status: ResolveStatus::Resolved,
            value: Some(value),
            source: Some(source),
            diagnostics: std::mem::take(&mut self.diagnostics),
            dependency_trace: std::mem::take(&mut self.trace),
            effective_context: self.effective_context.clone(),
        }
    }

    fn error_result(&mut self, diagnostics: Vec<Diagnostic>) -> ResolveResult {
        ResolveResult {
            contract_version: CONTRACT_VERSION.to_owned(),
            snapshot_revision: self.snapshot.revision.clone(),
            status: ResolveStatus::Error,
            value: None,
            source: None,
            diagnostics,
            dependency_trace: std::mem::take(&mut self.trace),
            effective_context: self.effective_context.clone(),
        }
    }

    fn diagnostic(&self, code: DiagnosticCode) -> Diagnostic {
        self.diagnostic_with_severity(code, Severity::Error)
    }

    fn diagnostic_with_severity(&self, code: DiagnosticCode, severity: Severity) -> Diagnostic {
        Diagnostic {
            code,
            severity,
            message: code.message().to_owned(),
            token_chain: self.trace.clone(),
            property_path: None,
        }
    }
}

impl DiagnosticCode {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::MissingToken => "MISSING_TOKEN",
            Self::MissingMode => "MISSING_MODE",
            Self::AliasCycle => "ALIAS_CYCLE",
            Self::AliasBudget => "ALIAS_BUDGET",
            Self::TypeMismatch => "TYPE_MISMATCH",
            Self::ScopeDenied => "SCOPE_DENIED",
            Self::ExpressionInvalid => "EXPRESSION_INVALID",
            Self::ExpressionBudget => "EXPRESSION_BUDGET",
            Self::ContextBudget => "CONTEXT_BUDGET",
            Self::DivisionByZero => "DIVISION_BY_ZERO",
            Self::ContextConflict => "CONTEXT_CONFLICT",
            Self::StaleRevision => "STALE_REVISION",
            Self::UnknownExtension => "UNKNOWN_EXTENSION",
            Self::UnsupportedFeature => "UNSUPPORTED_FEATURE",
            Self::NumericRange => "NUMERIC_RANGE",
            Self::InvalidSnapshot => "INVALID_SNAPSHOT",
            Self::InvalidPlan => "INVALID_PLAN",
            Self::DependencyOrder => "DEPENDENCY_ORDER",
        }
    }
}

fn compose_context(context: &ResolutionContext) -> Result<ContextMap, DiagnosticCode> {
    if context.ancestors_nearest_first.len() > 32 {
        return Err(DiagnosticCode::ContextBudget);
    }
    let mut effective = context.document.clone();
    effective.extend(context.page.clone());
    for ancestor in context.ancestors_nearest_first.iter().rev() {
        effective.extend(ancestor.clone());
    }
    effective.extend(context.explicit.clone());
    if effective.len() > MAX_CONTEXT_DIMENSIONS {
        return Err(DiagnosticCode::ContextBudget);
    }
    if effective
        .iter()
        .any(|(key, value)| key.is_empty() || value.is_empty())
    {
        return Err(DiagnosticCode::UnsupportedFeature);
    }
    Ok(effective)
}

fn context_matches(required: &ContextMap, effective: &ContextMap) -> bool {
    required
        .iter()
        .all(|(key, value)| effective.get(key).is_some_and(|actual| actual == value))
}

fn select_value<'a>(
    definition: &'a TokenDefinition,
    context: &ContextMap,
) -> Result<&'a crate::contract::TokenValueDefinition, DiagnosticCode> {
    let matches: Vec<_> = definition
        .values
        .iter()
        .filter(|value| context_matches(&value.when, context))
        .collect();
    let specificity = matches
        .iter()
        .map(|value| value.when.len())
        .max()
        .ok_or(DiagnosticCode::MissingMode)?;
    let mut winners = matches
        .into_iter()
        .filter(|value| value.when.len() == specificity);
    let selected = winners.next().ok_or(DiagnosticCode::MissingMode)?;
    if winners.next().is_some() {
        return Err(DiagnosticCode::ContextConflict);
    }
    Ok(selected)
}

fn validate_snapshot(snapshot: &TokenSnapshot) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let mut add = |code: DiagnosticCode| {
        if !errors
            .iter()
            .any(|diagnostic: &Diagnostic| diagnostic.code == code)
        {
            errors.push(Diagnostic {
                code,
                severity: Severity::Error,
                message: code.message().to_owned(),
                token_chain: Vec::new(),
                property_path: None,
            });
        }
    };
    if snapshot.contract_version != CONTRACT_VERSION || snapshot.document_namespace.is_empty() {
        add(DiagnosticCode::UnsupportedFeature);
    }
    if snapshot.context_dimensions.len() > MAX_CONTEXT_DIMENSIONS {
        add(DiagnosticCode::ContextBudget);
    }
    let mut set_ids = HashSet::new();
    for set in &snapshot.sets {
        if !set_ids.insert(set.id) {
            add(DiagnosticCode::InvalidSnapshot);
        }
    }
    let mut active_ids = HashSet::new();
    for id in &snapshot.active_set_ids {
        if !active_ids.insert(*id) || !set_ids.contains(id) {
            add(DiagnosticCode::InvalidSnapshot);
        }
    }
    let mut collection_ids = HashSet::new();
    for collection in &snapshot.collections {
        if !collection_ids.insert(collection.id)
            || collection.mode_dimension.is_empty()
            || collection.modes.is_empty()
            || !collection
                .modes
                .iter()
                .any(|mode| mode.id == collection.default_mode_id)
        {
            add(DiagnosticCode::InvalidSnapshot);
        }
        let mut mode_ids = HashSet::new();
        let mut options = HashSet::new();
        for mode in &collection.modes {
            if !mode_ids.insert(mode.id) || !options.insert(mode.option.clone()) {
                add(DiagnosticCode::InvalidSnapshot);
            }
        }
    }
    let mut dimension_names = HashSet::new();
    for dimension in &snapshot.context_dimensions {
        if dimension.name.is_empty()
            || !dimension_names.insert(dimension.name.clone())
            || dimension.options.is_empty()
        {
            add(DiagnosticCode::InvalidSnapshot);
        }
        let unique: HashSet<_> = dimension.options.iter().collect();
        if unique.len() != dimension.options.len() {
            add(DiagnosticCode::InvalidSnapshot);
        }
    }
    let mut definition_keys = HashSet::new();
    let mut identity_types: BTreeMap<(String, uuid::Uuid), TokenType> = BTreeMap::new();
    let mut identity_collection: BTreeMap<(String, uuid::Uuid), Option<uuid::Uuid>> =
        BTreeMap::new();
    for token in &snapshot.tokens {
        let key = (token.namespace.clone(), token.id, token.set_id);
        let identity = (token.namespace.clone(), token.id);
        if token.namespace.is_empty()
            || token.name.is_empty()
            || token.values.is_empty()
            || !definition_keys.insert(key)
        {
            add(DiagnosticCode::InvalidSnapshot);
        }
        if let Some(previous) = identity_types.insert(identity.clone(), token.token_type.clone()) {
            if previous != token.token_type {
                add(DiagnosticCode::TypeMismatch);
            }
        }
        if let Some(previous) = identity_collection.insert(identity.clone(), token.collection_id) {
            if previous != token.collection_id {
                add(DiagnosticCode::InvalidSnapshot);
            }
        }
        if token
            .collection_id
            .is_some_and(|id| !collection_ids.contains(&id))
            || token.set_id.is_some_and(|id| !set_ids.contains(&id))
        {
            add(DiagnosticCode::InvalidSnapshot);
        }
        for value in &token.values {
            if value.when.len() > MAX_CONTEXT_DIMENSIONS {
                add(DiagnosticCode::ContextBudget);
            }
            let mut nodes = 0;
            if let Err(code) = validate_expression_shape(&value.expression, 1, &mut nodes) {
                add(code);
            }
        }
    }
    errors
}

fn validate_expression_shape(
    expression: &Expression,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), DiagnosticCode> {
    *nodes += 1;
    if *nodes > MAX_EXPRESSION_NODES || depth > MAX_EXPRESSION_DEPTH {
        return Err(DiagnosticCode::ExpressionBudget);
    }
    match expression {
        Expression::Literal { value } => value.validate(),
        Expression::Reference { target } => {
            if target.namespace.is_empty() {
                Err(DiagnosticCode::ExpressionInvalid)
            } else {
                Ok(())
            }
        }
        Expression::Operation {
            operator,
            arguments,
        } => {
            let valid_count = match operator {
                ExpressionOperator::Subtract | ExpressionOperator::Divide => arguments.len() == 2,
                ExpressionOperator::Clamp => arguments.len() == 3,
                _ => arguments.len() >= 2,
            };
            if !valid_count {
                return Err(DiagnosticCode::ExpressionInvalid);
            }
            for argument in arguments {
                validate_expression_shape(argument, depth + 1, nodes)?;
            }
            Ok(())
        }
    }
}

fn evaluate_operation(
    operator: ExpressionOperator,
    values: &[TypedValue],
) -> Result<TypedValue, DiagnosticCode> {
    if values.len() < 2 {
        return Err(DiagnosticCode::ExpressionInvalid);
    }
    let numeric: Vec<_> = values.iter().map(numeric_value).collect::<Result<_, _>>()?;
    let dimension_count = numeric.iter().filter(|value| value.unit.is_some()).count();
    match operator {
        ExpressionOperator::Add
        | ExpressionOperator::Subtract
        | ExpressionOperator::Min
        | ExpressionOperator::Max
        | ExpressionOperator::Clamp => {
            let first_unit = numeric[0].unit;
            if numeric.iter().any(|value| match (first_unit, value.unit) {
                (None, None) => false,
                (Some(left), Some(right)) => !units_compatible(left, right),
                _ => true,
            }) {
                return Err(DiagnosticCode::TypeMismatch);
            }
            let canonical: Vec<f64> = numeric
                .iter()
                .map(|value| canonical_number(value.value, value.unit))
                .collect::<Result<_, _>>()?;
            let output = match operator {
                ExpressionOperator::Add => canonical.iter().sum(),
                ExpressionOperator::Subtract => canonical[0] - canonical[1],
                ExpressionOperator::Min => canonical.iter().copied().fold(f64::INFINITY, f64::min),
                ExpressionOperator::Max => {
                    canonical.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                }
                ExpressionOperator::Clamp if canonical[0] <= canonical[2] => {
                    canonical[1].clamp(canonical[0], canonical[2])
                }
                ExpressionOperator::Clamp => return Err(DiagnosticCode::ExpressionInvalid),
                _ => unreachable!(),
            };
            result_for_unit(output, first_unit)
        }
        ExpressionOperator::Multiply => {
            if dimension_count > 1 {
                return Err(DiagnosticCode::TypeMismatch);
            }
            let output = numeric.iter().map(|value| value.value).product();
            let unit = numeric.iter().find_map(|value| value.unit);
            let normalized = canonical_number(output, unit)?;
            result_for_unit(normalized, unit)
        }
        ExpressionOperator::Divide => {
            let numerator = numeric[0];
            let denominator = numeric[1];
            if denominator.value == 0.0 {
                return Err(DiagnosticCode::DivisionByZero);
            }
            match (numerator.unit, denominator.unit) {
                (None, None) => Ok(TypedValue::Number {
                    value: numerator.value / denominator.value,
                }),
                (Some(unit), None) => {
                    let canonical =
                        canonical_number(numerator.value, Some(unit))? / denominator.value;
                    result_for_unit(canonical, Some(unit))
                }
                (Some(left_unit), Some(right_unit)) if units_compatible(left_unit, right_unit) => {
                    let left = canonical_number(numerator.value, Some(left_unit))?;
                    let right = canonical_number(denominator.value, Some(right_unit))?;
                    Ok(TypedValue::Number {
                        value: left / right,
                    })
                }
                _ => Err(DiagnosticCode::TypeMismatch),
            }
        }
    }
}

#[derive(Clone, Copy)]
struct NumericValue {
    value: f64,
    unit: Option<DimensionUnit>,
}

fn numeric_value(value: &TypedValue) -> Result<NumericValue, DiagnosticCode> {
    match value {
        TypedValue::Number { value } => Ok(NumericValue {
            value: *value,
            unit: None,
        }),
        TypedValue::Dimension { value, unit } => Ok(NumericValue {
            value: *value,
            unit: Some(*unit),
        }),
        _ => Err(DiagnosticCode::TypeMismatch),
    }
}

fn canonical_number(value: f64, unit: Option<DimensionUnit>) -> Result<f64, DiagnosticCode> {
    let value = match unit {
        Some(DimensionUnit::S) => value * 1_000.0,
        _ => value,
    };
    validate_number(value)?;
    Ok(value)
}

fn units_compatible(left: DimensionUnit, right: DimensionUnit) -> bool {
    left == right
        || matches!(
            (left, right),
            (DimensionUnit::Ms, DimensionUnit::S) | (DimensionUnit::S, DimensionUnit::Ms)
        )
}

fn result_for_unit(value: f64, unit: Option<DimensionUnit>) -> Result<TypedValue, DiagnosticCode> {
    validate_number(value)?;
    match unit {
        Some(DimensionUnit::Px) => Ok(TypedValue::Dimension {
            value,
            unit: DimensionUnit::Px,
        }),
        Some(DimensionUnit::Ms | DimensionUnit::S) => Ok(TypedValue::Dimension {
            value,
            unit: DimensionUnit::Ms,
        }),
        None => Ok(TypedValue::Number { value }),
    }
}
