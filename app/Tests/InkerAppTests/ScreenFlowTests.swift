import Testing
@testable import InkerKit

/// Everything lives in the popover: the boards, the three onboarding steps
/// and settings are screens of one 360x500 surface, and a pending drop puts
/// the move-or-copy card in front of whichever screen is up.
struct ScreenFlowTests {
    private func completed() -> MemoryStore {
        let store = MemoryStore()
        var state = OnboardingState(store: store)
        state.markComplete()
        return store
    }

    @Test func firstLaunchStartsAtStepOne() {
        let flow = ScreenFlow(onboarding: OnboardingState(store: MemoryStore()))
        #expect(flow.screen == .onboarding(step: 1))
        #expect(flow.needsAttentionAtLaunch, "the popover opens by itself for onboarding")
    }

    @Test func aCompletedOnboardingStartsOnTheBoard() {
        let flow = ScreenFlow(onboarding: OnboardingState(store: completed()))
        #expect(flow.screen == .board)
        #expect(!flow.needsAttentionAtLaunch)
    }

    @Test func theEnvironmentOverrideForcesOnboarding() {
        // INKER_ONBOARDING=1
        let flow = ScreenFlow(onboarding: OnboardingState(store: completed()), forceOnboarding: true)
        #expect(flow.screen == .onboarding(step: 1))
        #expect(flow.needsAttentionAtLaunch)
    }

    @Test func nextAndBackWalkTheThreeSteps() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: MemoryStore()))
        flow.next()
        #expect(flow.screen == .onboarding(step: 2))
        flow.next()
        #expect(flow.screen == .onboarding(step: 3))
        flow.next()
        #expect(flow.screen == .onboarding(step: 3), "there is no step 4; Done leaves")
        flow.back()
        #expect(flow.screen == .onboarding(step: 2))
        flow.back()
        #expect(flow.screen == .onboarding(step: 1))
        flow.back()
        #expect(flow.screen == .onboarding(step: 1), "step 1 has no Back")
    }

    @Test func nextAndBackDoNothingOffTheSteps() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: completed()))
        flow.next()
        #expect(flow.screen == .board)
        flow.back()
        #expect(flow.screen == .board)
    }

    @Test func doneMarksOnboardingCompleteAndReturnsToTheBoard() {
        let store = MemoryStore()
        var flow = ScreenFlow(onboarding: OnboardingState(store: store))
        flow.next()
        flow.next()
        flow.finishOnboarding()
        #expect(flow.screen == .board)
        #expect(!OnboardingState(store: store).needsOnboarding)
        #expect(ScreenFlow(onboarding: OnboardingState(store: store)).screen == .board, "the next launch skips it")
    }

    @Test func theGearOpensSettingsAndClosesThemAgain() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: completed()))
        flow.toggleSettings()
        #expect(flow.screen == .settings)
        flow.toggleSettings()
        #expect(flow.screen == .board)
    }

    @Test func settingsGoesBackToTheBoard() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: completed()))
        flow.openSettings()
        #expect(flow.screen == .settings)
        flow.closeSettings()
        #expect(flow.screen == .board)
    }

    @Test func closingSettingsWhenTheyAreNotOpenChangesNothing() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: MemoryStore()))
        flow.next()
        flow.closeSettings()
        #expect(flow.screen == .onboarding(step: 2))
    }

    @Test func theGearDuringOnboardingOpensSettings() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: MemoryStore()))
        flow.next()
        flow.toggleSettings()
        #expect(flow.screen == .settings)
        flow.toggleSettings()
        #expect(flow.screen == .board)
    }

    @Test func runSetupAgainStartsAtStepOne() {
        let store = completed()
        var flow = ScreenFlow(onboarding: OnboardingState(store: store))
        flow.openSettings()
        flow.runSetupAgain()
        #expect(flow.screen == .onboarding(step: 1))
        flow.next()
        flow.next()
        flow.finishOnboarding()
        #expect(flow.screen == .board)
        #expect(!OnboardingState(store: store).needsOnboarding)
    }

    @Test func aPendingDropShowsTheCardInFrontOfAnyScreen() {
        let screens: [PopoverScreen] = [.board, .onboarding(step: 1), .onboarding(step: 2), .onboarding(step: 3), .settings]
        for screen in screens {
            var flow = ScreenFlow(onboarding: OnboardingState(store: completed()))
            flow.show(screen)
            #expect(flow.visible(dropPending: true) == .dropCard, "\(screen)")
            #expect(flow.visible(dropPending: false) == .screen(screen), "\(screen)")
        }
    }

    @Test func theScreenResumesWhenTheDropIsAnswered() {
        var flow = ScreenFlow(onboarding: OnboardingState(store: MemoryStore()))
        flow.next()
        #expect(flow.visible(dropPending: true) == .dropCard)
        #expect(flow.screen == .onboarding(step: 2), "the step underneath is kept")
        #expect(flow.visible(dropPending: false) == .screen(.onboarding(step: 2)))
    }

    @Test func showClampsTheStep() {
        // The snapshot mode sets screens directly.
        var flow = ScreenFlow(onboarding: OnboardingState(store: completed()))
        flow.show(.onboarding(step: 3))
        #expect(flow.screen == .onboarding(step: 3))
        flow.show(.onboarding(step: 9))
        #expect(flow.screen == .onboarding(step: ScreenFlow.stepCount))
        flow.show(.onboarding(step: 0))
        #expect(flow.screen == .onboarding(step: 1))
    }
}
